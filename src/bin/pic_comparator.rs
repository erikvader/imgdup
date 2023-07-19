use error_stack::{bail, report, IntoReport, ResultExt};
use std::path::{Path, PathBuf};

use clap::Parser;
use image::RgbImage;
use imgdup::{
    error_stack_utils::IntoReportChangeContext,
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
    imgdup::plot::bar_chart(cli.pictures, &[("test", 1), ("asd", 2), ("omg", 11)])
        .change_context(ImgError)?;
    // imgdup::plot::bar_chart(cli.pictures, &[]).change_context(ImgError)?;
    Ok(())
}
