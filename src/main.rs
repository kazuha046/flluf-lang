#![allow(linker_messages)]
mod build;
mod cli;
mod codegen;
mod error;
mod link;
mod resolver;
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

        Commands::New { name } => {
            let dir = std::env::current_dir()?.join(&name);

            if dir.exists() {
                anyhow::bail!("directory `{}` already exists", name);
            }

            std::fs::create_dir_all(&dir)?;

            let toml = format!(
                r#"[package]
name = "{name}"
version = "0.1.0"
"#
            );

            let main_fll = r#"use System;

void Main():
{
	Log("Hello, world!");
}
"#;

            std::fs::write(dir.join("init.toml"), toml)?;
            std::fs::write(dir.join("main.fll"), main_fll)?;

            eprintln!("Created project `{name}` at {}", dir.display());
        }
    }

    Ok(())
}
