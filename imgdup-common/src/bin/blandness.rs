use std::path::PathBuf;

use clap::Parser;
use color_eyre::eyre::{self, Context};
use imgdup_common::bin_common::{args::one_color::OneColor, init::init_eyre};

#[derive(Parser)]
#[command()]
/// Calculates blandness of a picture
struct Cli {
    #[command(flatten)]
    one_color_args: OneColor,

    /// The image file to use
    inputs: Vec<PathBuf>,
}

fn main() -> eyre::Result<()> {
    init_eyre()?;
    let cli = Cli::parse();

    for input in cli.inputs {
        let pic = image::open(&input)
            .wrap_err_with(|| format!("Could not open {:?}", input))?
            .to_rgb8();

        let one_color = cli.one_color_args.one_color(&pic);
        let is_one_color = cli.one_color_args.is_value_too_one_color(one_color);

        let input = input.display();
        println!("{input}: one={one_color}({is_one_color})");
    }

    Ok(())
}
