use std::{collections::HashMap, ffi::OsString, path::PathBuf};

use clap::Parser;
use color_eyre::eyre::{self, Context};
use image::RgbImage;
use imgdup::{
    bktree::BKTree,
    common::{init_logger_and_eyre, VidSrc},
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
    #[arg(long, short = 'r', default_value = "0000_the_new_one")]
    reference_file: OsString, // TODO: assert filename, i.e., no slashes nor has_prefix. Or exactly one normal component?
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

// TODO: log what is is doing
fn main() -> eyre::Result<()> {
    init_logger_and_eyre()?;
    let cli = Cli::parse();

    let mut repo_entry = Entry::open(".").wrap_err("failed to open the current dir")?;

    let ref_path = remove_dot_dot(
        std::fs::read_link(cli.reference_file)
            .wrap_err("failed to read the referemce file")?,
    );
    assert!(is_simple_relative(&ref_path));

    let root = cli
        .database_file
        .parent()
        .ok_or_else(|| eyre::eyre!("database file path doesn't have a parent path"))?;

    let mut tree =
        BKTree::<VidSrc>::from_file(&cli.database_file).wrap_err_with(|| {
            format!(
                "Failed to open database at: {}",
                cli.database_file.display()
            )
        })?;

    // TODO: extract function
    let ref_frames: Vec<Frame> = {
        let mut ref_frames = Vec::new();
        tree.for_each(|hash, vidsrc| {
            if vidsrc.path() == ref_path {
                ref_frames.push(Frame {
                    vidsrc: vidsrc.clone(),
                    hash,
                });
            }
        })?;
        ref_frames
    };

    // TODO: extract  function
    let collisions: Vec<Collision> = {
        let mut collisions = Vec::new();
        for ref_frame in ref_frames {
            tree.find_within(
                ref_frame.hash,
                cli.similarity_threshold,
                |other_hash, other_vidsrc| {
                    collisions.push(Collision {
                        reference: ref_frame.clone(),
                        other: Frame {
                            vidsrc: other_vidsrc.clone(),
                            hash: other_hash,
                        },
                    })
                },
            )?;
        }
        collisions
    };

    tree.close()?;

    // TODO: extract function
    let images: HashMap<VidSrc, RgbImage> = {
        let mut images = HashMap::new();
        for collision in collisions.iter() {
            for vidsrc in [&collision.reference.vidsrc, &collision.other.vidsrc] {
                if !images.contains_key(vidsrc) {
                    let full_path = root.join(vidsrc.path());
                    let mut extractor =
                        FrameExtractor::new(&full_path).wrap_err_with(|| {
                            format!(
                                "failed to open frame extractor for {}",
                                full_path.display()
                            )
                        })?;

                    // TODO: don't start from the beginning again
                    for collision in collisions.iter() {
                        for vidsrc2 in
                            [&collision.reference.vidsrc, &collision.other.vidsrc]
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
                }
            }
        }
        images
    };

    // TODO: extract function
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
        entry.create_text_file("collided_hash", other.hash.to_base64())?;
        entry.create_text_file("reference_hash", reference.hash.to_base64())?;
        entry.create_text_file(
            "hash_distance",
            format!(
                "{} <= {}",
                other.hash.distance_to(reference.hash),
                cli.similarity_threshold
            ),
        )?;
    }

    Ok(())
}
