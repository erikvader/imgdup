use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use clap::Parser;
use color_eyre::eyre::{self, Context};
use image::RgbImage;
use imgdup::{
    bin_common::init::{init_eyre, init_logger},
    bktree::{mmap::bktree::BKTree, source_types::video_source::VidSrc},
    frame_extractor::frame_extractor::FrameExtractor,
    imghash::{
        hamming::Hamming,
        preproc::{PreprocArgs, PreprocCli},
        similarity::{SimiArgs, SimiCli},
    },
    utils::{
        repo::Entry,
        simple_path::{SimplePath, SimplePathBuf},
    },
};

#[derive(Parser, Debug)]
#[command()]
/// Dump debug information on a dup
struct Cli {
    #[command(flatten)]
    simi_args: SimiCli,

    #[command(flatten)]
    preproc_args: PreprocCli,

    /// Path to the database to use
    #[arg(long, short = 'd', default_value = "../../imgdup.db")]
    database_file: PathBuf,

    /// The file to compare against all other
    #[arg(long, short = 'r', default_value = "./0000_the_new_one")]
    reference_file: PathBuf,

    /// The repo entry directory to debug, or the repo itself if the flag `all` is given.
    #[arg(long, short = 'e', default_value = ".")]
    entry_dir: PathBuf,

    #[arg(long, short = 'A', default_value_t = false)]
    all: bool,
}

#[derive(Clone)]
struct Frame {
    hash: Hamming,
    vidsrc: VidSrc,
}

struct Collision {
    reference: Frame,
    other: Frame,
}

struct PreprocImage {
    original: RgbImage,
    preproc: RgbImage,
}

fn main() -> eyre::Result<()> {
    init_eyre()?;
    init_logger(None)?;
    let cli = Cli::parse();

    let simi_args = cli.simi_args.as_args();
    let preproc_args = cli.preproc_args.as_args();

    let ref_path: SimplePathBuf = {
        let link = std::fs::read_link(cli.reference_file)
            .wrap_err("failed to read the reference file")?;
        SimplePathBuf::unresolve(link)
            .wrap_err("the link at the reference file is not simple")?
    };

    let root = cli
        .database_file
        .parent()
        .ok_or_else(|| eyre::eyre!("database file path doesn't have a parent path"))?;

    let mut repo_entry =
        Entry::open(&cli.entry_dir).wrap_err("failed to open the current dir")?;
    let tree = BKTree::<VidSrc>::from_file(&cli.database_file).wrap_err_with(|| {
        format!(
            "Failed to open database at: {}",
            cli.database_file.display()
        )
    })?;

    log::info!("Extracting frames for the reference video...");
    let ref_frames: Vec<Frame> = extract_frames(&tree, &ref_path)?;
    log::info!("Done!");

    log::info!("Finding the collisions for all reference frames...");
    let collisions: Vec<Collision> =
        find_collisions(&ref_frames, &ref_path, &tree, &simi_args)?;
    log::info!("Done!");

    drop(tree);

    log::info!(
        "Extracting the frames for all {} collisions...",
        collisions.len()
    );
    let images: HashMap<VidSrc, RgbImage> = read_images_from_videos(&collisions, root)?;
    log::info!("Done!");

    log::info!("Preprocessing all images...");
    let images: HashMap<VidSrc, PreprocImage> = preproc_images(images, &preproc_args);
    log::info!("Done!");

    log::info!("Saving everything to the repo entry...");
    save_collisions(&collisions, &mut repo_entry, root, images, &simi_args)?;
    log::info!("Done!");

    Ok(())
}

fn save_collisions(
    collisions: &[Collision],
    repo_entry: &mut Entry,
    root: &Path,
    images: HashMap<VidSrc, PreprocImage>,
    simi_args: &SimiArgs,
) -> eyre::Result<()> {
    for Collision { other, reference } in collisions {
        let mut entry = repo_entry
            .sub_entry("collision")
            .wrap_err("failed to create collision sub entry")?;

        entry.create_link("collided_with", root.join(other.vidsrc.path()))?;

        let PreprocImage {
            original: other_org,
            preproc: other_pre,
        } = images.get(&other.vidsrc).expect("should exist");
        let PreprocImage {
            original: ref_org,
            preproc: ref_pre,
        } = images.get(&reference.vidsrc).expect("should exist");

        entry.create_jpg("collided_frame", other_org)?;
        entry.create_jpg("reference_frame", ref_org)?;
        entry.create_jpg("collided_frame_preproc", other_pre)?;
        entry.create_jpg("reference_frame_preproc", ref_pre)?;

        entry.create_text_file(
            "collided_timestamp",
            other.vidsrc.frame_pos().to_string(),
        )?;
        entry.create_text_file(
            "reference_timestamp",
            reference.vidsrc.frame_pos().to_string(),
        )?;

        entry.create_text_file("collided_mirror", other.vidsrc.mirrored().to_string())?;
        entry.create_text_file(
            "reference_mirror",
            reference.vidsrc.mirrored().to_string(),
        )?;

        entry.create_text_file("collided_hash", other.hash.to_base64())?;
        entry.create_text_file("reference_hash", reference.hash.to_base64())?;
        entry.create_text_file(
            "hash_distance",
            format!(
                "{} <= {}",
                other.hash.distance_to(reference.hash),
                simi_args.threshold()
            ),
        )?;
    }
    Ok(())
}

// TODO: parallelize somehow, with rayon?
fn read_images_from_videos(
    collisions: &[Collision],
    root: &Path,
) -> eyre::Result<HashMap<VidSrc, RgbImage>> {
    let mut images = HashMap::new();
    for collision in collisions.iter() {
        for vidsrc in [&collision.reference.vidsrc, &collision.other.vidsrc] {
            if !images.contains_key(vidsrc) {
                let full_path = root.join(vidsrc.path());
                log::info!("Opening: {}", full_path.display());
                let mut extractor =
                    FrameExtractor::new(&full_path).wrap_err_with(|| {
                        format!(
                            "failed to open frame extractor for {}",
                            full_path.display()
                        )
                    })?;

                // TODO: don't start from the beginning again
                for collision in collisions.iter() {
                    for vidsrc2 in [&collision.reference.vidsrc, &collision.other.vidsrc]
                    {
                        if vidsrc2.path() == vidsrc.path()
                            && !images.contains_key(vidsrc2)
                        {
                            extractor
                                .seek_to(vidsrc2.frame_pos())
                                .wrap_err("failed to seek")?;

                            let Some((_, img)) = extractor.next().wrap_err("failed to get frame")? else {
                                eyre::bail!("should have returned an image");
                            };

                            images.insert(vidsrc2.clone(), img);
                        }
                    }
                }

                log::info!("Done with: {}", full_path.display());
            }
        }
    }
    Ok(images)
}

fn find_collisions(
    ref_frames: &[Frame],
    ref_path: &SimplePath,
    tree: &BKTree<VidSrc>,
    simi_args: &SimiArgs,
) -> eyre::Result<Vec<Collision>> {
    let mut collisions = Vec::new();
    for ref_frame in ref_frames {
        tree.find_within(
            ref_frame.hash,
            simi_args.threshold(),
            |other_hash, other_vidsrc| {
                if ref_path != other_vidsrc.path() {
                    collisions.push(Collision {
                        reference: ref_frame.clone(),
                        other: Frame {
                            vidsrc: other_vidsrc.deserialize(),
                            hash: other_hash,
                        },
                    })
                }
            },
        )?;
    }
    Ok(collisions)
}

fn extract_frames(
    tree: &BKTree<VidSrc>,
    ref_path: &SimplePath,
) -> eyre::Result<Vec<Frame>> {
    let mut ref_frames = Vec::new();
    tree.for_each(|hash, vidsrc| {
        if vidsrc.path() == ref_path {
            ref_frames.push(Frame {
                vidsrc: vidsrc.deserialize(),
                hash,
            });
        }
    })?;
    Ok(ref_frames)
}

fn preproc_images(
    images: HashMap<VidSrc, RgbImage>,
    preproc_args: &PreprocArgs,
) -> HashMap<VidSrc, PreprocImage> {
    images
        .into_iter()
        .map(|(vidsrc, img)| {
            (
                vidsrc,
                PreprocImage {
                    preproc: preproc_args.preprocess(&img),
                    original: img,
                },
            )
        })
        .collect()
}
