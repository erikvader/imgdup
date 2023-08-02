use color_eyre::eyre;
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
/// Hash pictures and compare them
struct Cli {
    /// A folder with pictures in
    pictures: PathBuf,
}

fn main() -> eyre::Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();
    imgdup::plot::bar_chart(cli.pictures, &[("test", 1), ("asd", 2), ("omg", 11)])?;
    // imgdup::plot::bar_chart(cli.pictures, &[]).change_context(ImgError)?;
    Ok(())
}
