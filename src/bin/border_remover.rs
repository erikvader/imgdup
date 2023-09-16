use std::path::PathBuf;

use clap::Parser;
use color_eyre::eyre::{self, Context};
use image::{DynamicImage, GenericImageView};
use imgdup::imgutils::{self, RemoveBordersConf};

#[derive(Parser)]
#[command()]
/// Performs different stages of removing the borders of an image
struct Cli {
    /// Don't remove the borders, run maskify instead
    #[arg(long)]
    maskify: bool,

    /// All gray values below this becomes black
    #[arg(long, short = 't', default_value_t = imgutils::DEFAULT_MASKIFY_THRESHOLD)]
    maskify_threshold: u8,

    /// A mask line can contain this many percent of white and still be considered black
    #[arg(long, short = 'w', default_value_t = imgutils::DEFAULT_BORDER_MAX_WHITES)]
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
        let cropped = imgutils::remove_borders(
            &input,
            &RemoveBordersConf::default()
                .maskify_threshold(cli.maskify_threshold)
                .maximum_whites(cli.maximum_whites),
        );
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
