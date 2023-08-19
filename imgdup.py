#!/bin/python

# Pillow==7.2.0
from PIL import Image
import numpy as np

# ImageHash==4.1.0
import imagehash
import cv2
import os
import pickle
import gzip
import random
import shutil
import sys
from shutil import rmtree
from itertools import chain

# TODO: byt ut ImageHash-objekten mot ngt bättre?

DBFILE = "imgdupdb.gz"
SAVEBACK = 400
IGNOREDIR = "_IMGDUP_IGNORE_"


def count_elements(l):
    if not l:
        return []

    sort = sorted(l)
    result = {}
    i = 0
    j = 0
    while i < len(sort):
        while j < len(sort) and sort[i] == sort[j]:
            j += 1
        result[sort[i]] = j - i
        i = j

    return result


def all_files(*dirs):
    res = set()
    for d in dirs:
        with os.scandir(d) as fs:
            for f in fs:
                if f.is_file():
                    res.add(f.path)
    return res


def watermark_getbbox(mask_arr, maximum_whites=0.03):
    assert mask_arr.size > 0

    def find_edges(axis, length):
        whites = mask_arr.sum(axis=axis) / length
        keep = np.flatnonzero(whites > maximum_whites)
        if keep.size >= 1:
            return keep[0], keep[-1] + 1
        else:
            return 0, 0

    width = mask_arr.shape[1]
    height = mask_arr.shape[0]
    left, right = find_edges(0, width)
    top, bottom = find_edges(1, height)

    if left == right or top == bottom:
        return None
    return left, top, right, bottom


def maskify(img, low_threshold=20):
    mapper = lambda x: 0 if x <= low_threshold else 1
    grayscale = img.convert("L")
    return grayscale.point(mapper, mode="1")


def remove_borders_debug(img):
    mask = maskify(img)
    bbox = watermark_getbbox(np.array(mask))
    return bbox, mask


def remove_borders(img):
    bbox, _ = remove_borders_debug(img)
    if bbox is None:
        return None
    return img.crop(bbox)


def extractImages(path, intervals, window=10, prefix=None):
    print("\r{} [{}/{}]".format(prefix, 0, 0), end="")

    # pylint: disable=c-extension-no-member
    vidcap = cv2.VideoCapture(path)

    frames = vidcap.get(cv2.CAP_PROP_FRAME_COUNT)
    fps = vidcap.get(cv2.CAP_PROP_FPS)
    duration = (frames / fps) * 1000

    window *= 1000
    middle = duration * 0.5
    start_position = max(0, middle - window)
    end_position = min(duration, middle + window)
    actual_window = end_position - start_position
    positions = [n * actual_window + start_position for n in intervals]

    images = []

    count = 0
    for position in positions:
        count += 1
        if prefix is not None:
            print("\r{} [{}/{}]".format(prefix, count, len(positions)), end="")
        vidcap.set(cv2.CAP_PROP_POS_MSEC, position)
        success, image = vidcap.read()
        if not success:
            print("\nfailed to read image")
            continue
        with Image.fromarray(cv2.cvtColor(image, cv2.COLOR_BGR2RGB)) as pil_image:
            borderless = remove_borders(pil_image)
            if borderless is not None:
                images.append(borderless)
        # cv2.imwrite("{}/test_{}.jpg".format(folder, count), image)
        # count += 1
    if prefix is not None:
        print("")
    vidcap.release()

    return images


def videoHashes(path, ignore_hashes=None, prefix=None):
    keyframes = [0, 0.5, 1]
    # keyframes = [0, 0.05, 0.1, 0.23, 0.25, 0.27, 0.47]
    # keyframes = keyframes + [0.5] + [1-x for x in keyframes]
    keyframes.extend(random.random() for _ in range(17))
    keyframes.sort()
    images = []
    extra_images = []

    # TODO: refactor to return an optional hash?
    def hash_img(img, imgs):
        h = imagehash.dhash(img)
        if ignore_hashes is not None and any(i - h <= 3 for i in ignore_hashes):
            print("ignoring an image")
            # img.close() TODO: figure out when to close
            return
        imgs.append((img, h))

    for img in extractImages(path, keyframes, window=20, prefix=prefix):
        hash_img(img, images)
        flipped = img.transpose(Image.Transpose.FLIP_LEFT_RIGHT)
        hash_img(flipped, extra_images)

    return images, extra_images


class BKTree:
    def __init__(self, h, v):
        self.key = h
        self.values = [v]
        self.children = {}

    def add(self, h, v):
        d = h - self.key
        if d == 0:
            self.values.append(v)
        elif d in self.children:
            self.children[d].add(h, v)
        else:
            self.children[d] = BKTree(h, v)

    def search(self, h, t, retlist=None):
        retlist = retlist if retlist is not None else []
        d = h - self.key
        if d <= t:
            retlist.append((self.key, self.values))

        if d > 0:
            for i in range(d - t, d + t + 1):
                if i in self.children:
                    self.children[i].search(h, t, retlist)

        return retlist

    def elements(self):
        for v in self.values:
            yield (self.key, v)

        for c in self.children.values():
            for x in c.elements():
                yield x


class DB:
    def __init__(self):
        self.tree = None
        self.files = []

    @classmethod
    def from_db(cls, otherdb):
        db = cls()
        seen = {}

        for f, h in otherdb.elements():
            if f in seen:
                db.tree.add(h, seen[f])
            else:
                db.add(f, [h])
                seen[f] = len(db.files) - 1

        return db

    def add(self, path, hashes):
        self.files.append(path)
        i = len(self.files) - 1
        for h in hashes:
            if self.tree is None:
                self.tree = BKTree(h, i)
            else:
                self.tree.add(h, i)

    def remove(self, paths):
        for i, f in enumerate(self.files):
            if f is not None and f in paths:
                self.files[i] = None

    def search_all(self, hashes, threshold=3):
        if self.tree is None:
            return []

        matches = []
        for h in hashes:
            for key, files in self.tree.search(h, threshold):
                for i in files:
                    if self.files[i] is None:
                        continue
                    matches.append((key, self.files[i]))

        return matches

    def get_files(self):
        return {f for f in self.files if f is not None}

    def save(self, path):
        _, ext = os.path.splitext(path)
        if ext == ".gz":
            file_fun = lambda f: gzip.GzipFile(f, "w")
        else:
            file_fun = lambda f: open(f, "wb")

        with file_fun(path) as f:
            pickle.dump((self.tree, self.files), f)

    def load(self, path):
        _, ext = os.path.splitext(path)
        if ext == ".gz":
            file_fun = lambda f: gzip.GzipFile(f, "r")
        else:
            file_fun = lambda f: open(f, "rb")

        with file_fun(path) as f:
            self.tree, self.files = pickle.load(f)

    def elements(self):
        if self.tree is None:
            return

        for k, v in self.tree.elements():
            if self.files[v] is not None:
                yield (self.files[v], k)

    def procent_dead(self):
        dead = 0
        total = 0
        for f in self.files:
            total += 1
            if f is None:
                dead += 1
        return dead / total


def find_ignores():
    if not os.path.isdir(IGNOREDIR):
        print("no ignoredir found, skipping...")
        return []

    print("generating hashes for ignored images...")
    res = []
    for f in all_files(IGNOREDIR):
        with Image.open(f) as img:
            res.append(imagehash.dhash(img))
    print("done generating hashes!")
    return res


def save_frame(path, image, hashh):
    folders = os.path.join("_IMGDUP_FRAMES_", path)
    os.makedirs(folders, exist_ok=True)
    image.save(os.path.join(folders, str(hashh)), "JPEG")


def find_dups(db, new_files, ignore_hashes):
    count = 0
    for fi, path in enumerate(new_files):
        images, extra_images = videoHashes(path, ignore_hashes, f"{fi} {path}")

        dups = {}
        saved_images = []
        saved_counts = []
        for i, h in chain(images, extra_images):
            # save_frame(path, i, h) # NOTE: generates too many files that im not using yet
            ds = db.search_all([h])
            if ds:
                saved_images.append(i)
                saved_counts.append(len(ds))
                for _, img in ds:
                    dups[img] = 1 + dups.get(img, 0)

        if dups:
            count += 1
            dup_dir = os.path.join("_DUPS_", str(count))
            os.mkdir(dup_dir)
            for i, d in enumerate(dups.keys()):
                os.symlink(
                    os.path.relpath(d, dup_dir),
                    os.path.join(dup_dir, f"2_dup_{i} [{dups[d]}]"),
                )

            os.symlink(
                os.path.relpath(path, dup_dir), os.path.join(dup_dir, "0_the_new_one")
            )

            for i, (img, c) in enumerate(zip(saved_images, saved_counts)):
                img.save(
                    os.path.join(dup_dir, "1_keyframe_{} [{}]".format(i, c)), "JPEG"
                )

        for i, _ in chain(images, extra_images):
            i.close()

        db.add(path, (h for _, h in images))

        if (fi + 1) % SAVEBACK == 0:
            db.save(DBFILE)
            print("saved dbfile")


def folder_empty(fold):
    return not os.listdir(fold)


def main():
    if not folder_empty("_DUPS_"):
        print("please clear _DUPS_ first")
        return

    db = DB()
    if os.path.isfile(DBFILE):
        shutil.copyfile(DBFILE, DBFILE + ".bak")
        db.load(DBFILE)

    # dump contents
    # for f, h in db.elements():
    #     print(f"{h}\t{f}")
    # return

    afiles = all_files("_ARCHIVE_", "_BUFFER_", "_BUFFER_1")
    dfiles = db.get_files()

    removed_files = dfiles - afiles
    db.remove(removed_files)
    for rf in removed_files:
        x = os.path.join("_IMGDUP_FRAMES_", rf)
        if os.path.isdir(x):
            rmtree(x)

    new_files = afiles - dfiles
    find_dups(db, new_files, find_ignores())

    print("wasted space: {:.1f}%".format(db.procent_dead() * 100))
    if db.procent_dead() >= 0.05:
        print("rebuilding db...")
        db = DB.from_db(db)

    db.save(DBFILE)


def run_tests():
    for in_path in all_files(os.path.join("_IMGDUP_TESTS_", "input")):
        in_filename, ext = os.path.splitext(os.path.basename(in_path))
        # pylint: disable=cell-var-from-loop
        outname = lambda extra: os.path.join(
            "_IMGDUP_TESTS_",
            "output",
            in_filename + "_" + extra + ext,
        )
        with Image.open(in_path) as img:
            bbox, mask = remove_borders_debug(img)
            mask.save(outname("2mask"))
            if bbox is not None:
                img.crop(bbox).save(outname("1cropped"))


if __name__ == "__main__":
    if len(sys.argv) >= 2 and sys.argv[1] == "--test":
        run_tests()
    else:
        main()
