use std::path::PathBuf;

use clap::Parser;
use color_eyre::eyre::{self, Context};
use imgdup::{
    bktree::BKTree,
    common::{init_eyre, init_logger, VidSrc},
};

#[derive(Parser, Debug)]
#[command()]
/// Edit an existing database
struct Cli {
    /// Path to the database to use
    #[arg(long, short = 'f')]
    database_file: PathBuf,

    /// Goals to execute
    #[arg(value_parser = goal_parser, required = true)]
    goals: Vec<Goal>,
}

#[derive(Debug, Clone)]
enum Goal {
    Stats,
    Rebuild,
    Cleanse { dir: PathBuf },
}

fn goal_parser(s: &str) -> Result<Goal, String> {
    let parts: Vec<_> = s.split(':').collect();
    match &parts[..] {
        &["stats"] => Ok(Goal::Stats),
        &["rebuild"] => Ok(Goal::Rebuild),
        &["cleanse", arg1] => Ok(Goal::Cleanse { dir: arg1.into() }),
        _ => Err(format!("Failed to parse goal '{s}', unrecognized")),
    }
}

fn main() -> eyre::Result<()> {
    init_eyre()?;
    init_logger(None)?;
    let cli = Cli::parse();

    let mut tree =
        BKTree::<VidSrc>::from_file(&cli.database_file).wrap_err_with(|| {
            format!(
                "failed to open the database at: {}",
                cli.database_file.display()
            )
        })?;

    for goal in cli.goals {
        log::info!("Performing goal: {goal:?}");
        match goal {
            Goal::Stats => goal_stats(&mut tree),
            Goal::Rebuild => goal_rebuild(&mut tree),
            Goal::Cleanse { dir } => todo!(),
        }
        .wrap_err_with(|| format!("failed to perform goal '{goal:?}'"))?;
        log::info!("Done with goal: {goal:?}");
    }

    tree.close().wrap_err("Failed to close the tree")?;

    Ok(())
}

fn goal_stats(tree: &mut BKTree<VidSrc>) -> eyre::Result<()> {
    let (alive, dead) = tree.count_nodes().wrap_err("failed to count nodes")?;
    let total = alive + dead;
    log::info!("Stats:");
    log::info!("  Alive nodes = {alive}");
    log::info!("  Dead  nodes = {dead}");
    log::info!("  Total nodes = {total}");
    Ok(())
}

fn goal_rebuild(tree: &mut BKTree<VidSrc>) -> eyre::Result<()> {
    let (alive, dead) = tree.rebuild().wrap_err("failed to rebuild")?;
    let total = alive + dead;
    log::info!("Stats after rebuild:");
    log::info!("  Alive nodes = {alive}");
    log::info!("  Dead  nodes = {dead}");
    log::info!("  Total nodes = {total}");
    Ok(())
}
