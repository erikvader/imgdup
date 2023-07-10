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

#[derive(thiserror::Error, Debug)]
#[error("Failed to hash images or something")]
struct ImgError;

fn main() -> error_stack::Result<(), ImgError> {
    let cli = Cli::parse();
    Ok(())
}
