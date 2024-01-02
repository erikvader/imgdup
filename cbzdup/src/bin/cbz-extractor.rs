use std::path::PathBuf;

use cbzdup::cbz_reader::CbzReader;
use clap::Parser;
use color_eyre::eyre;
use imgdup_common::bin_common::init::{init_eyre, init_logger};

#[derive(Parser)]
#[command()]
/// Extract all images from a CBZ file
struct Cli {
    /// Where to place the frames as images
    #[arg(long)]
    outdir: PathBuf,

    /// The cbz file to extract from
    cbzfile: PathBuf,
}

fn main() -> eyre::Result<()> {
    init_eyre()?;
    init_logger(None)?;
    let cli = Cli::parse();

    if !cli.outdir.is_dir() {
        std::fs::create_dir(&cli.outdir)?;
    }

    let mut extractor = CbzReader::new(cli.cbzfile)?;
    let mut i = 0;
    while let Some(img) = extractor.next()? {
        let filename = format!("page_{i}.jpg");
        i += 1;
        println!("Writing {:?}", filename);
        img.save(cli.outdir.join(filename))?;
    }

    Ok(())
}
