#![allow(linker_messages)]
mod build;
mod cli;
mod codegen;
mod link;
mod syntax;

use anyhow::{Context, Result};
use clap::Parser as ClapParser;
use cli::{Cli, Commands};
use std::process::Command;

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.cmd {
        Commands::Build { file, output } => {
            build::build(&file, output.as_deref())?;
        }

        Commands::Run { file } => {
            let exe = build::build(&file, None)?;

            let status = Command::new(&exe)
                .status()
                .with_context(|| format!("failed to run {}", exe.display()))?;

            let code = status.code().unwrap_or(1);

            eprintln!("program exited with code {code}");

            if code != 0 {
                std::process::exit(code);
            }
        }
    }

    Ok(())
}
