use std::{path::PathBuf, time::Duration};

use clap::Parser;
use error_stack::{report, IntoReport, ResultExt};
use imgdup::frame_extractor::FrameExtractor;
use rand::{thread_rng, Rng};

#[derive(Parser)]
#[command()]
struct Cli {
    /// The video file to extract from
    videofile: PathBuf,

    /// Where to save the random frame
    output: PathBuf,
}

#[derive(thiserror::Error, Debug)]
#[error("Failed to extract a random frame")]
struct RandError;

fn main() -> error_stack::Result<(), RandError> {
    let cli = Cli::parse();

    let mut extractor = FrameExtractor::new(cli.videofile).change_context(RandError)?;
    let len = extractor.approx_length();
    let target = Duration::from_secs(thread_rng().gen_range(0..=len.as_secs()));

    extractor.seek_forward(target).change_context(RandError)?;
    let img = match extractor.next().change_context(RandError)? {
        Some((_, img)) => img,
        None => {
            // if the seek seeked too far
            extractor.seek_to_beginning().change_context(RandError)?;
            let (_, img) = extractor
                .next()
                .change_context(RandError)?
                .expect("there are no frames in this video at all");
            img
        }
    };

    img.save(cli.output)
        .into_report()
        .change_context(RandError)?;

    Ok(())
}
