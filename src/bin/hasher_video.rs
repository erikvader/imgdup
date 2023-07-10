use std::path::{Path, PathBuf};

use clap::Parser;
use error_stack::{IntoReport, ResultExt};
use image::RgbImage;
use imgdup::{
    frame_extractor::FrameExtractor,
    imghash::{self, hamming::Hamming},
    imgutils,
};

#[derive(Parser)]
#[command()]
/// Hash frames from a video with slight modifications to get a feel for hash performance
struct Cli {
    /// Perform tests on different resolutions
    #[arg(long, value_delimiter = ',', value_name = "HEIGHTS", num_args = 1..)]
    resolution_test: Option<Vec<u32>>,

    /// Perform tests on different qualities (downscale upscale)
    #[arg(long, value_delimiter = ',', value_name = "HEIGHTS", num_args = 1..)]
    quality_test: Option<Vec<u32>>,

    /// Perform tests on this many subsequent frames
    #[arg(long)]
    consecutive_test: Option<u32>,

    /// How much to skip between consecutive frames
    #[arg(long, requires = "consecutive_test")]
    step: Option<humantime::Duration>,

    /// Perform a test on a flipped frame
    #[arg(long)]
    flip_test: bool,

    /// Offset in the video to grab the frame from
    #[arg(long)]
    offset: humantime::Duration,

    /// Save the generated frames here, if given
    #[arg(long)]
    outdir: Option<PathBuf>,

    /// The video file to extract from
    videofile: PathBuf,
}

#[derive(thiserror::Error, Debug)]
#[error("Failed to hash videos or something")]
struct VidError;

fn main() -> error_stack::Result<(), VidError> {
    let cli = Cli::parse();

    match &cli.outdir {
        Some(dir) if !dir.is_dir() => std::fs::create_dir(dir)
            .into_report()
            .change_context(VidError)?,
        _ => (),
    }

    let mut extractor = FrameExtractor::new(cli.videofile).change_context(VidError)?;
    extractor
        .seek_forward(cli.offset.into())
        .change_context(VidError)?;
    let (_, frame) = extractor
        .next()
        .change_context(VidError)?
        .expect("did not get a frame, seeked too far?");

    let frame_hash = imghash::hash(&frame);
    println!("The test frame has hash: {frame_hash}");

    write_image(cli.outdir.as_ref(), "reference.jpg", &frame)?;

    if let Some(heights) = cli.resolution_test {
        resolution_test(&frame, frame_hash, &heights, cli.outdir.as_ref())?;
    }

    if let Some(heights) = cli.quality_test {
        quality_test(&frame, frame_hash, &heights, cli.outdir.as_ref())?;
    }

    if let Some(times) = cli.consecutive_test {
        consecutive_test(
            &frame,
            frame_hash,
            times,
            cli.step,
            cli.outdir.as_ref(),
            &mut extractor,
        )?;
    }

    Ok(())
}

fn resolution_test(
    frame: &RgbImage,
    frame_hash: Hamming,
    heights: &[u32],
    outdir: Option<&PathBuf>,
) -> error_stack::Result<(), VidError> {
    for h in heights {
        let resized = imgutils::resize_keep_aspect_ratio(frame, *h);
        let filename = format!("resolution_{h}.jpg");
        write_image(outdir, filename, &resized)?;

        let resized_hash = imghash::hash(&resized);
        let dist = frame_hash.distance_to(resized_hash);

        println!("The distance to height={h} is {dist} ({resized_hash})");
    }

    Ok(())
}

fn quality_test(
    frame: &RgbImage,
    frame_hash: Hamming,
    heights: &[u32],
    outdir: Option<&PathBuf>,
) -> error_stack::Result<(), VidError> {
    for h in heights {
        let resized = imgutils::worsen_quality(frame, *h);
        let filename = format!("quality_{h}.jpg");
        write_image(outdir, filename, &resized)?;

        let resized_hash = imghash::hash(&resized);
        let dist = frame_hash.distance_to(resized_hash);

        println!("The distance to height={h} is {dist} ({resized_hash})");
    }

    Ok(())
}

fn consecutive_test(
    _frame: &RgbImage,
    _frame_hash: Hamming,
    times: u32,
    _step: Option<humantime::Duration>,
    _outdir: Option<&PathBuf>,
    _extractor: &mut FrameExtractor,
) -> error_stack::Result<(), VidError> {
    for _i in 1..=times {
        todo!(
            "what does this really say? It really depends on where in the video this is"
        );
    }
    Ok(())
}

fn write_image<P1, P2>(
    outdir: Option<P1>,
    filename: P2,
    image: &RgbImage,
) -> error_stack::Result<(), VidError>
where
    P1: AsRef<Path>,
    P2: AsRef<Path>,
{
    Ok(if let Some(outdir) = outdir {
        let outdir = outdir.as_ref();
        let filename = filename.as_ref();
        println!("Writing {filename:?}");
        image
            .save(outdir.join(filename))
            .into_report()
            .change_context(VidError)?;
    })
}
