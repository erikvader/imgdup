use std::path::{Path, PathBuf};

use clap::Parser;
use image::RgbImage;
use imgdup::{
    frame_extractor::FrameExtractor,
    imghash::{self, hamming::Hamming},
    imgutils,
};

#[derive(Parser)]
#[command()]
/// Hash frames frome a video with slight modifications to get a feel for hash performance
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

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match &cli.outdir {
        Some(dir) if !dir.is_dir() => std::fs::create_dir(dir)?,
        _ => (),
    }

    let mut extractor = FrameExtractor::new(cli.videofile)?;
    extractor.seek_forward(cli.offset.into())?;
    let (_, frame) = extractor
        .next()?
        .expect("did not get a frame, seeked too far?");

    let frame_hash = imghash::hash(&frame);
    println!("The test frame has hash: {frame_hash}");

    write_image(cli.outdir.as_ref(), "reference.jpg", &frame)?;

    if let Some(heights) = cli.resolution_test {
        resolution_test(&frame, frame_hash, &heights, cli.outdir.as_ref())?;
    }

    Ok(())
}

fn resolution_test(
    frame: &RgbImage,
    frame_hash: Hamming,
    heights: &[u32],
    outdir: Option<&PathBuf>,
) -> anyhow::Result<()> {
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

fn write_image<P1, P2>(
    outdir: Option<P1>,
    filename: P2,
    image: &RgbImage,
) -> anyhow::Result<()>
where
    P1: AsRef<Path>,
    P2: AsRef<Path>,
{
    Ok(if let Some(outdir) = outdir {
        let outdir = outdir.as_ref();
        let filename = filename.as_ref();
        println!("Writing {filename:?}");
        image.save(outdir.join(filename))?;
    })
}
