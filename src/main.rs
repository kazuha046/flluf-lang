#![allow(linker_messages)]
mod codegen;
mod syntax;

use anyhow::{Context, Result};
use clap::{Parser as ClapParser, Subcommand};
use std::path::PathBuf;
use std::process::Command;

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

            let code = status.code().unwrap_or(1);

            eprintln!("program exited with code {code}");

            if code != 0 {
                std::process::exit(code);
            }
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

    #[cfg(target_os = "windows")]
    let (obj_ext, exe_ext) = (".obj", ".exe");

    #[cfg(not(target_os = "windows"))]
    let (obj_ext, exe_ext) = (".o", "");

    let obj_path = build_dir.join(format!("{stem}{obj_ext}"));

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

    let exe_path = output.cloned().unwrap_or_else(|| {
        let name = format!("{stem}{exe_ext}");
        target.join(name)
    });

    #[cfg(target_os = "windows")]
    link_win(&obj_path, &exe_path)?;

    #[cfg(not(target_os = "windows"))]
    link_gcc(&obj_path, &exe_path)?;

    eprintln!("compiled to {}", exe_path.display());

    Ok(exe_path)
}

#[cfg(target_os = "windows")]
fn link_win(obj: &PathBuf, exe: &PathBuf) -> Result<()> {
    let obj_str = obj.display().to_string();
    let exe_str = exe.display().to_string();

    let mingw_names = &[
        "x86_64-w64-mingw32-gcc",
        "x86_64-w64-mingw32-ld",
        "mingw32-gcc",
    ];

    for mingw in mingw_names {
        if let Some(linker) = find_on_path(mingw) {
            let out = Command::new(linker)
                .arg(&obj_str)
                .arg("-o")
                .arg(&exe_str)
                .arg("-lkernel32")
                .arg("-nostdlib")
                .arg("-e")
                .arg("main")
                .output()
                .context(format!("running {}", mingw))?;

            if out.status.success() {
                return Ok(());
            }

            let err = String::from_utf8_lossy(&out.stderr);

            anyhow::bail!("{} failed:\n{err}", mingw);
        }
    }

    if let Some(link) = find_on_path("link.exe") {
        let out = Command::new(link)
            .arg("/NOLOGO")
            .arg(format!("/OUT:{exe_str}"))
            .arg("/SUBSYSTEM:CONSOLE")
            .arg("/NODEFAULTLIB")
            .arg("/ENTRY:main")
            .arg(&obj_str)
            .arg("kernel32.lib")
            .output()
            .context("running link.exe")?;

        if out.status.success() {
            return Ok(());
        }

        let err = String::from_utf8_lossy(&out.stderr);

        anyhow::bail!("link.exe failed:\n{err}");
    }

    let vs_path = get_vs_path().context(
        "No mingw linker found, link.exe not on PATH, and VS not found.\n\
         Install mingw-w64 (https://www.mingw-w64.org) or open a VS Developer Command Prompt.",
    )?;

    let vcvars = vs_path.join(r"VC\Auxiliary\Build\vcvarsall.bat");
    let vcvars_str = vcvars.display().to_string();

    let script = format!(
        "@echo off\r\n\
         call \"{vcvars_str}\" x64\r\n\
         if errorlevel 1 exit /b 1\r\n\
         link.exe /NOLOGO /OUT:\"{exe_str}\" /SUBSYSTEM:CONSOLE /NODEFAULTLIB /ENTRY:main \"{obj_str}\" kernel32.lib\r\n\
         exit /b %errorlevel%\r\n"
    );

    let script_path = std::env::temp_dir().join("flluf_link.bat");

    std::fs::write(&script_path, script.as_bytes()).context("writing linker script")?;

    let out = Command::new(&script_path)
        .output()
        .context("running VS linker script")?;

    let _ = std::fs::remove_file(&script_path);

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let out_ = String::from_utf8_lossy(&out.stdout);
        anyhow::bail!("VS linking failed.\nstdout:\n{out_}\nstderr:\n{err}");
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn get_vs_path() -> Result<PathBuf> {
    let vswhere = r"C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe";
    let vswhere = PathBuf::from(vswhere);

    if !vswhere.is_file() {
        anyhow::bail!("vswhere.exe not found at {}", vswhere.display());
    }

    let out = Command::new(&vswhere)
        .args([
            "-latest",
            "-property",
            "installationPath",
            "-format",
            "value",
        ])
        .output()
        .context("running vswhere.exe")?;

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        anyhow::bail!("vswhere failed:\n{err}");
    }

    let raw = String::from_utf8_lossy(&out.stdout);
    let path = raw.trim();

    if path.is_empty() {
        anyhow::bail!("vswhere returned no installation path");
    }

    let vs = PathBuf::from(path);

    if !vs.join(r"VC\Auxiliary\Build\vcvarsall.bat").exists() {
        anyhow::bail!(
            "VS found at {} but vcvarsall.bat is missing. Install \"Desktop development with C++\".",
            vs.display()
        );
    }

    Ok(vs)
}

#[cfg(target_os = "windows")]
fn find_on_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path).find_map(|dir| {
            let candidate = dir.join(name);
            candidate.is_file().then_some(candidate)
        })
    })
}

#[cfg(not(target_os = "windows"))]
fn link_gcc(obj: &PathBuf, exe: &PathBuf) -> Result<()> {
    let exe_str = exe.display().to_string();

    if exe_str.ends_with(".exe") {
        for mingw in &["x86_64-w64-mingw32-gcc", "x86_64-w64-mingw32-ld"] {
            let output = match Command::new(mingw)
                .arg(obj)
                .arg("-o")
                .arg(exe)
                .arg("-lkernel32")
                .arg("-nostdlib")
                .arg("-e")
                .arg("main")
                .output()
            {
                Ok(out) => out,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e).context(format!("spawning {mingw}")),
            };

            if output.status.success() {
                return Ok(());
            }

            let stderr = String::from_utf8_lossy(&output.stderr);

            for line in stderr.lines() {
                if line.contains("error:") || !line.contains("warning:") {
                    eprintln!("{line}");
                }
            }

            anyhow::bail!("{mingw} link failed");
        }

        anyhow::bail!(
            "no mingw cross-linker found (tried x86_64-w64-mingw32-gcc, x86_64-w64-mingw32-ld)"
        );
    }

    let cc = if cfg!(target_os = "linux") {
        "gcc"
    } else {
        "cc"
    };

    let output = Command::new(cc)
        .arg("-no-pie")
        .arg("-o")
        .arg(exe)
        .arg(obj)
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

    Ok(())
}
