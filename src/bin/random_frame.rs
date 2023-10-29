use std::{path::PathBuf, time::Duration};

use clap::Parser;
use color_eyre::eyre;
use imgdup::frame_extractor::frame_extractor::FrameExtractor;
use rand::{thread_rng, Rng};

#[derive(Parser)]
#[command()]
/// Extracts a random frame from a video file
struct Cli {
    /// The video file to extract from
    videofile: PathBuf,

    /// Where to save the random frame
    output: PathBuf,
}

fn main() -> eyre::Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();

    let mut extractor = FrameExtractor::new(cli.videofile)?;
    let len = extractor.approx_length();
    let target = Duration::from_secs(thread_rng().gen_range(0..=len.as_secs()));

    extractor.seek_forward(target)?;
    let img = match extractor.next()? {
        Some((_, img)) => img,
        None => {
            // if the seek seeked too far
            extractor.seek_to_beginning()?;
            let (_, img) = extractor
                .next()?
                .expect("there are no frames in this video at all");
            img
        }
    };

    img.save(cli.output)?;

    Ok(())
}
