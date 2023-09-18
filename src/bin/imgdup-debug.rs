use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use clap::Parser;
use color_eyre::eyre::{self, Context};
use image::RgbImage;
use imgdup::{
    bktree::BKTree,
    common::{init_eyre, init_logger, VidSrc},
    frame_extractor::FrameExtractor,
    fsutils::{is_simple_relative, remove_dot_dot},
    imghash::{
        self,
        hamming::{Distance, Hamming},
    },
    repo::Entry,
};

#[derive(Parser, Debug)]
#[command()]
/// Dump debug information on a dup
struct Cli {
    /// Maximum distance for two images to be considered equal
    #[arg(long, default_value_t = imghash::DEFAULT_SIMILARITY_THRESHOLD)]
    similarity_threshold: Distance,

    /// Path to the database to use
    #[arg(long, short = 'f', default_value = "../../imgdup.db")]
    database_file: PathBuf,

    /// The file to compare against all other
    #[arg(long, short = 'r', default_value = "./0000_the_new_one")]
    reference_file: PathBuf,
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

fn main() -> eyre::Result<()> {
    init_eyre()?;
    init_logger(None)?;
    let cli = Cli::parse();

    let ref_path = remove_dot_dot(
        std::fs::read_link(cli.reference_file)
            .wrap_err("failed to read the referemce file")?,
    );
    assert!(is_simple_relative(&ref_path));

    let root = cli
        .database_file
        .parent()
        .ok_or_else(|| eyre::eyre!("database file path doesn't have a parent path"))?;

    let mut repo_entry = Entry::open(".").wrap_err("failed to open the current dir")?;
    let mut tree =
        BKTree::<VidSrc>::from_file(&cli.database_file).wrap_err_with(|| {
            format!(
                "Failed to open database at: {}",
                cli.database_file.display()
            )
        })?;

    log::info!("Extracting frames for the reference video...");
    let ref_frames: Vec<Frame> = extract_frames(&mut tree, &ref_path)?;
    log::info!("Done!");

    log::info!("Finding the collisions for all reference frames...");
    let collisions: Vec<Collision> =
        find_collisions(&ref_frames, &ref_path, &mut tree, cli.similarity_threshold)?;
    log::info!("Done!");

    tree.close()?;

    log::info!("Extracting the frames for all collisions...");
    let images: HashMap<VidSrc, RgbImage> = read_images_from_videos(&collisions, root)?;
    log::info!("Done!");

    log::info!("Saving everything to the repo entry...");
    save_collisions(
        &collisions,
        &mut repo_entry,
        root,
        images,
        cli.similarity_threshold,
    )?;
    log::info!("Done!");

    Ok(())
}

fn save_collisions(
    collisions: &[Collision],
    repo_entry: &mut Entry,
    root: &Path,
    images: HashMap<VidSrc, RgbImage>,
    similarity_threshold: Distance,
) -> eyre::Result<()> {
    for Collision { other, reference } in collisions {
        let mut entry = repo_entry
            .sub_entry("collision")
            .wrap_err("failed to create collision sub entry")?;

        entry.create_link_relative("collided_with", root.join(other.vidsrc.path()))?;
        entry.create_jpg(
            "collided_frame",
            images.get(&other.vidsrc).expect("should exist"),
        )?;
        entry.create_jpg(
            "reference_frame",
            images.get(&reference.vidsrc).expect("should exist"),
        )?;
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
                similarity_threshold
            ),
        )?;
    }
    Ok(())
}

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
    ref_path: &Path,
    tree: &mut BKTree<VidSrc>,
    similarity_threshold: Distance,
) -> eyre::Result<Vec<Collision>> {
    let mut collisions = Vec::new();
    for ref_frame in ref_frames {
        tree.find_within(
            ref_frame.hash,
            similarity_threshold,
            |other_hash, other_vidsrc| {
                if ref_path != other_vidsrc.path() {
                    collisions.push(Collision {
                        reference: ref_frame.clone(),
                        other: Frame {
                            vidsrc: other_vidsrc.clone(),
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
    tree: &mut BKTree<VidSrc>,
    ref_path: &Path,
) -> eyre::Result<Vec<Frame>> {
    let mut ref_frames = Vec::new();
    tree.for_each(|hash, vidsrc| {
        if vidsrc.path() == ref_path {
            ref_frames.push(Frame {
                vidsrc: vidsrc.clone(),
                hash,
            });
        }
    })?;
    Ok(ref_frames)
}
