use clap::{Parser as ClapParser, Subcommand};
use std::path::PathBuf;

#[derive(ClapParser)]
#[command(name = "flluf", version)]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Build {
        file: PathBuf,
        #[arg(short)]
        output: Option<PathBuf>,
    },
    Run {
        file: PathBuf,
    },
}
