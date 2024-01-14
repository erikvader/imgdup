use std::path::PathBuf;

use clap::Parser;
use color_eyre::eyre::{self, Context};
use imgdup_common::bin_common::{
    args::preproc::{Preproc, PreprocError},
    init::init_eyre,
};

#[derive(Parser)]
#[command()]
/// Preprocesses images before hashing
struct Cli {
    #[command(flatten)]
    preproc: Preproc,

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

        let reason = match cli.preproc.check(&pic) {
            Ok(_) => "Ok",
            Err(PreprocError::Empty) => "Empty",
            Err(PreprocError::TooOneColor) => "TooOneColor",
        };

        let input = input.display();
        println!("{input}: {reason}");
    }

    Ok(())
}
