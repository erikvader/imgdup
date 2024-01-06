use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use clap::Parser;
use color_eyre::eyre::{self, Context};
use image::RgbImage;
use imgdup_common::{
    bin_common::{
        args::{
            preproc::{PreprocArgs, PreprocCli},
            similarity::{SimiArgs, SimiCli},
        },
        init::{init_eyre, init_logger},
    },
    utils::repo::{Entry, Repo},
};
use videodup::{
    debug_info::{self, Collision, DEBUG_INFO_FILENAME},
    frame_extractor::FrameExtractor,
    video_source::VidSrc,
};

#[derive(Parser, Debug)]
#[command()]
/// Dump debug information on a dup
struct Cli {
    #[command(flatten)]
    simi_args: SimiCli,

    #[command(flatten)]
    preproc_args: PreprocCli,

    /// Path to the root, where all relative paths originate
    #[arg(long, short = 'd', default_value = "../../")]
    root: PathBuf,

    /// The repo entry directory to debug, or the repo itself if the flag `all` is given.
    #[arg(long, short = 'e', default_value = ".")]
    entry_dir: PathBuf,

    /// Debug all entries instead of just the current one
    #[arg(long, short = 'A', default_value_t = false)]
    all: bool,

    /// Maximum number of collisions to write
    #[arg(long, default_value_t = 100)]
    max_collisions: usize,
}

struct PreprocImage {
    original: RgbImage,
    preproc: RgbImage,
}

fn main() -> eyre::Result<()> {
    init_eyre()?;
    init_logger(None)?;
    let cli = Cli::parse();

    let simi_args = cli.simi_args.to_args();
    let preproc_args = cli.preproc_args.to_args();

    let root = cli.root;

    let entries = if cli.all {
        let repo = Repo::new(&cli.entry_dir)
            .wrap_err("failed to open a repo at the entry dir")?;
        repo.entries()?
    } else {
        let repo_entry =
            Entry::open(&cli.entry_dir).wrap_err("failed to open the entry dir")?;
        vec![repo_entry]
    };

    for repo_entry in entries {
        execute_on_entry(
            cli.max_collisions,
            &simi_args,
            &preproc_args,
            &root,
            repo_entry,
        )?;
    }

    Ok(())
}

fn execute_on_entry(
    max_collisions: usize,
    simi_args: &SimiArgs,
    preproc_args: &PreprocArgs,
    root: &Path,
    mut repo_entry: Entry,
) -> eyre::Result<()> {
    log::info!("Creating debug info at: {}", repo_entry.path().display());

    log::info!("Reading debug info file...");
    let mut collisions = repo_entry
        .read_file(DEBUG_INFO_FILENAME, |buf| debug_info::read_from(buf))
        .wrap_err("failed to read the debug info file")?
        .collisions;
    eyre::ensure!(!collisions.is_empty(), "The debug info file was empty");
    log::info!("Done! It had {} collisions", collisions.len());

    if collisions.len() > max_collisions {
        log::warn!("Got more than {max_collisions} collisions, will truncate");
        collisions.truncate(max_collisions);
    }

    log::info!("Extracting the frames for all collisions from the video files...",);
    let images: HashMap<VidSrc, RgbImage> = read_images_from_videos(&collisions, root)?;
    log::info!("Done! Got {} images", images.len());

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

                            let Some((_, img)) =
                                extractor.next().wrap_err("failed to get frame")?
                            else {
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
