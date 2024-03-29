use std::path::PathBuf;

use clap::Parser;
use color_eyre::eyre::{self, Context};
use image::{imageops::grayscale, DynamicImage, GenericImageView};
use imgdup_common::bin_common::args::remove_borders::RemoveBorders;

#[derive(Parser)]
#[command()]
/// Performs different stages of removing the borders of an image
struct Cli {
    #[command(flatten)]
    border_args: RemoveBorders,

    /// Don't remove the borders, run maskify instead
    #[arg(long)]
    maskify: bool,

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
        cli.border_args.maskify(grayscale(&input)).0.into()
    } else {
        let cropped = cli.border_args.remove_borders(&input);
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
