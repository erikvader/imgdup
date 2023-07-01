use std::path::PathBuf;

use clap::Parser;
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

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if !cli.outdir.is_dir() {
        std::fs::create_dir(&cli.outdir)?;
    }

    let mut extractor = FrameExtractor::new(cli.videofile)?;
    extractor.seek_forward(cli.offset.into())?;
    for i in 1..=cli.num {
        match extractor.next() {
            Ok(Some((ts, img))) => {
                let frame_filename = format!(
                    "{}/frame_{}_{}.jpg",
                    cli.outdir.to_string_lossy(),
                    i,
                    ts.to_string()
                );
                println!("Writing {}", frame_filename);
                img.save(frame_filename)?;
            }
            Ok(None) => break,
            Err(e) => return Err(e.into()),
        }
        extractor.seek_forward(cli.step.into())?;
    }

    Ok(())
}