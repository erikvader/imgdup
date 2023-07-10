use std::path::PathBuf;

use clap::Parser;
use error_stack::{IntoReport, ResultExt};
use imgdup::frame_extractor::FrameExtractor;

#[derive(Parser)]
#[command()]
struct Cli {
    /// How long between each frame
    #[arg(long, default_value = "1s")]
    step: humantime::Duration,

    /// Where to start in the file
    #[arg(long, default_value = "0s")]
    offset: humantime::Duration,

    /// How many frames to extract in total
    #[arg(long, default_value_t = 10)]
    num: usize,

    /// Where to place the frames as images
    #[arg(long)]
    outdir: PathBuf,

    /// The video file to extract from
    videofile: PathBuf,
}

#[derive(thiserror::Error, Debug)]
#[error("Failed to extract frames")]
struct ExtError;

fn main() -> error_stack::Result<(), ExtError> {
    let cli = Cli::parse();

    if !cli.outdir.is_dir() {
        std::fs::create_dir(&cli.outdir)
            .into_report()
            .change_context(ExtError)?;
    }

    let mut extractor = FrameExtractor::new(cli.videofile).change_context(ExtError)?;
    extractor
        .seek_forward(cli.offset.into())
        .change_context(ExtError)?;
    for i in 1..=cli.num {
        match extractor.next().change_context(ExtError)? {
            Some((ts, img)) => {
                let frame_filename = format!("frame_{}_{}.jpg", i, ts.to_string());
                println!("Writing {:?}", frame_filename);
                img.save(cli.outdir.join(frame_filename))
                    .into_report()
                    .change_context(ExtError)?;
            }
            None => break,
        }
        extractor
            .seek_forward(cli.step.into())
            .change_context(ExtError)?;
    }

    Ok(())
}
