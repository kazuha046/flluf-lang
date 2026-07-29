mod codegen;
mod syntax;

use anyhow::{Context, Result};
use clap::{Parser as ClapParser, Subcommand};
use std::path::PathBuf;
use std::process::Command;

const RUNTIME_C: &str = r##"#include <stdio.h>
#include <stdlib.h>

long long __flluf_log_i64(long long val) {
    printf("%lld\n", val); return val;
}
double __flluf_log_f64(double val) {
    printf("%f\n", val); return val;
}
const char* __flluf_log_ptr(const char* val) {
    printf("%s\n", val); return val;
}
void __flluf_exit(long long code) {
    if (code != 0) {
        fprintf(stderr, "exit code %lld\n", code);
    }
    exit(code);
}
"##;

#[derive(ClapParser)]
#[command(name = "flluf", version)]
struct Cli {
    #[command(subcommand)]
    cmd: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Build {
        file: PathBuf,
        #[arg(short)]
        output: Option<PathBuf>,
    },
    Run {
        file: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.cmd {
        Commands::Build { file, output } => {
            build(&file, output.as_ref())?;
        }

        Commands::Run { file } => {
            let exe = build(&file, None)?;

            let status = Command::new(&exe)
                .status()
                .with_context(|| format!("failed to run {}", exe.display()))?;

            std::process::exit(status.code().unwrap_or(1));
        }
    }

    Ok(())
}

fn build(input: &PathBuf, output: Option<&PathBuf>) -> Result<PathBuf> {
    let cwd = PathBuf::from(".");
    let parent = input.parent().unwrap_or(&cwd);
    let target = parent.join("target");
    let build_dir = target.join("build");

    std::fs::create_dir_all(&build_dir).context("create target/build")?;

    let stem = input.file_stem().unwrap().to_str().unwrap();
    let obj_path = build_dir.join(format!("{stem}.o"));
    let rt_path = build_dir.join("runtime.c");

    let src =
        std::fs::read_to_string(input).with_context(|| format!("read {}", input.display()))?;

    let toks: Vec<syntax::token::Token> = syntax::lexer::Lexer::new(&src).collect();

    let program = syntax::parser::Parser::new(toks)
        .parse()
        .with_context(|| format!("failed to parse {}", input.display()))?;

    let obj_data = codegen::compile(&program)
        .with_context(|| format!("failed to compile {}", input.display()))?;

    std::fs::write(&obj_path, &obj_data)
        .with_context(|| format!("write {}", obj_path.display()))?;

    std::fs::write(&rt_path, RUNTIME_C).with_context(|| format!("write {}", rt_path.display()))?;

    let exe_path = output.cloned().unwrap_or_else(|| target.join(stem));

    let cc = if cfg!(target_os = "linux") {
        "gcc"
    } else {
        "cc"
    };

    let output = Command::new(cc)
        .arg("-no-pie")
        .arg("-o")
        .arg(&exe_path)
        .arg(&obj_path)
        .arg(&rt_path)
        .arg("-lm")
        .output()
        .with_context(|| format!("{cc} link failed"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);

        for line in stderr.lines() {
            if line.contains("error:") || !line.contains("warning:") {
                eprintln!("{line}");
            }
        }

        anyhow::bail!("link failed");
    }

    eprintln!("compiled to {}", exe_path.display());

    Ok(exe_path)
}
