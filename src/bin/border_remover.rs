use std::path::PathBuf;

use clap::{Args, Parser};
use color_eyre::eyre::{self, Context};
use image::{DynamicImage, GenericImageView, RgbImage};
use imgdup::{frame_extractor::FrameExtractor, imgutils};
use rand::{thread_rng, Rng};

#[derive(Parser)]
#[command()]
/// Performs different stages of removing the borders of an image
struct Cli {
    /// Don't remove the borders, run maskify instead
    #[arg(long)]
    maskify: bool,

    /// All gray values below this becomes black
    #[arg(long, short = 't', default_value_t = 20)]
    maskify_threshold: u8,

    /// A mask line can contain this many percent of white and still be considered black
    #[arg(long, short = 'w', default_value_t = 0.03)]
    maximum_whites: f64,

    /// Where to save the resulting image
    #[arg(long, short = 'o')]
    output: Option<PathBuf>,

    /// The image file to remove borders of
    input: PathBuf,
}

fn main() -> eyre::Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();

    let input = image::open(&cli.input)
        .wrap_err_with(|| format!("Could not open {:?}", cli.input))?
        .to_rgb8();
    println!("before:  {:?}", input.bounds());

    let output: DynamicImage = if cli.maskify {
        imgutils::maskify(&input, cli.maskify_threshold).into()
    } else {
        let cropped =
            imgutils::remove_borders(&input, cli.maskify_threshold, cli.maximum_whites);
        println!("cropped: {:?}", cropped.bounds());
        cropped.to_image().into()
    };

    println!("after:   {:?}", output.bounds());

    if let Some(output_path) = cli.output {
        output
            .save(&output_path)
            .wrap_err_with(|| format!("Could not save to {output_path:?}"))?;
    }

    Ok(())
}
