use std::path::PathBuf;

use clap::Parser;
use color_eyre::eyre::{self, Context};
use imgdup::{
    bin_common::init::init_eyre,
    utils::imgutils::{BlackMaskCli, BlandnessCli, RemoveBordersCli},
};

#[derive(Parser)]
#[command()]
/// Calculates blandness of a picture
struct Cli {
    #[command(flatten)]
    bland_args: BlandnessCli,

    #[command(flatten)]
    black_args: BlackMaskCli,

    #[command(flatten)]
    border_args: RemoveBordersCli,

    /// The image file to use
    inputs: Vec<PathBuf>,
}

fn main() -> eyre::Result<()> {
    init_eyre()?;
    let cli = Cli::parse();

    let bland_args = cli.bland_args.to_args();
    let black_args = cli.black_args.to_args();
    let border_args = cli.border_args.to_args();

    for input in cli.inputs {
        let pic = image::open(&input)
            .wrap_err_with(|| format!("Could not open {:?}", input))?
            .to_rgb8();

        let blandness = bland_args.blandness(&pic);
        let is_bland = bland_args.is_value_bland(blandness);

        let mask = border_args.maskify(&pic);
        let blackness = black_args.blackness(&mask);
        let is_black = black_args.is_value_too_black(blackness);

        let input = input.display();
        println!("{input}: Bland={blandness}({is_bland}), Black={blackness}({is_black})");
    }

    Ok(())
}
