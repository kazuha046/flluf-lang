use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

const RUNTIME_C: &str = include_str!("runtime.c");

fn write_runtime_c(obj: &Path) -> Result<PathBuf> {
    let dir = obj
        .parent()
        .with_context(|| format!("no parent directory for {}", obj.display()))?;
    
    let path = dir.join("flluf_runtime.c");

    std::fs::write(&path, RUNTIME_C).context("write flluf_runtime.c")?;

    Ok(path)
}

#[cfg(target_os = "windows")]
pub fn link_win(obj: &Path, exe: &Path) -> Result<()> {
    let obj_str = obj.display().to_string();
    let exe_str = exe.display().to_string();
    let runtime_c = write_runtime_c(obj)?;

    if let Some(link) = find_on_path("link.exe") {
        if let Some(runtime_obj) = compile_runtime_obj(&runtime_c, obj)? {
            let out = Command::new(link)
                .arg("/NOLOGO")
                .arg(format!("/OUT:{exe_str}"))
                .arg("/SUBSYSTEM:CONSOLE")
                .arg("/ENTRY:main")
                .arg(&obj_str)
                .arg(&runtime_obj)
                .arg("kernel32.lib")
                .output()
                .context("running link.exe")?;

            if out.status.success() {
                return Ok(());
            }

            anyhow::bail!("link.exe failed:\n{}", String::from_utf8_lossy(&out.stderr));
        }
    }

    for mingw in &["x86_64-w64-mingw32-gcc", "x86_64-w64-mingw32-ld"] {
        if let Some(linker) = find_on_path(mingw) {
            let mut cmd = Command::new(linker);

            cmd.arg(&obj_str)
                .arg("-o")
                .arg(&exe_str)
                .arg("-lkernel32")
                .arg("-nostdlib")
                .arg("-e")
                .arg("main");

            if *mingw == "x86_64-w64-mingw32-gcc" {
                cmd.arg(&runtime_c);
            } else {
                let Some(runtime_obj) = compile_runtime_obj(&runtime_c, obj)? else {
                    continue;
                };

                cmd.arg(&runtime_obj);
            }

            let out = cmd.output().context(format!("running {mingw}"))?;

            if out.status.success() {
                return Ok(());
            }

            anyhow::bail!("{mingw} failed:\n{}", String::from_utf8_lossy(&out.stderr));
        }
    }

    let vs_path = get_vs_path().context(
        "no linker found (tried link.exe, x86_64-w64-mingw32-gcc, x86_64-w64-mingw32-ld).\n\
         Install mingw-w64 (https://www.mingw-w64.org) or open a VS Developer Command Prompt.",
    )?;

    let runtime_obj = obj
        .parent()
        .with_context(|| format!("no parent directory for {}", obj.display()))?
        .join("flluf_runtime.obj");

    let vcvars = vs_path.join(r"VC\Auxiliary\Build\vcvarsall.bat");

    let script = format!(
        "@echo off\r\n\
         call \"{}\" x64\r\n\
         if errorlevel 1 exit /b 1\r\n\
         cl.exe /nologo /c /O2 /Fo\"{}\" \"{}\"\r\n\
         if errorlevel 1 exit /b 1\r\n\
         link.exe /NOLOGO /OUT:\"{exe_str}\" /SUBSYSTEM:CONSOLE /ENTRY:main \"{obj_str}\" \"{}\" kernel32.lib\r\n\
         exit /b %errorlevel%\r\n",
        vcvars.display(),
        runtime_obj.display(),
        runtime_c.display(),
        runtime_obj.display()
    );

    let script_path = std::env::temp_dir().join("flluf_link.bat");

    std::fs::write(&script_path, script.as_bytes()).context("writing linker script")?;

    let out = Command::new(&script_path)
        .output()
        .context("running VS linker script")?;

    let _ = std::fs::remove_file(&script_path);

    if !out.status.success() {
        anyhow::bail!(
            "VS linking failed.\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn compile_runtime_obj(runtime_c: &Path, obj: &Path) -> Result<Option<PathBuf>> {
    let out = obj
        .parent()
        .map(|p| p.join("flluf_runtime.obj"))
        .with_context(|| format!("no parent directory for {}", obj.display()))?;

    if let Some(cl) = find_on_path("cl.exe") {
        let result = Command::new(cl)
            .arg("/nologo")
            .arg("/c")
            .arg("/O2")
            .arg(format!("/Fo{}", out.display()))
            .arg(runtime_c)
            .output();

        if let Ok(o) = result {
            if o.status.success() {
                return Ok(Some(out));
            }
        }
    }

    for mingw in &["x86_64-w64-mingw32-gcc"] {
        if let Some(gcc) = find_on_path(mingw) {
            let result = Command::new(gcc)
                .arg("-c")
                .arg(runtime_c)
                .arg("-o")
                .arg(&out)
                .output();

            if let Ok(o) = result {
                if o.status.success() {
                    return Ok(Some(out));
                }
            }
        }
    }

    Ok(None)
}

#[cfg(target_os = "windows")]
fn get_vs_path() -> Result<PathBuf> {
    let vswhere =
        PathBuf::from(r"C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe");

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
        anyhow::bail!("vswhere failed:\n{}", String::from_utf8_lossy(&out.stderr));
    }

    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();

    if path.is_empty() {
        anyhow::bail!("vswhere returned no installation path");
    }

    let vs = PathBuf::from(&path);

    if !vs.join(r"VC\Auxiliary\Build\vcvarsall.bat").exists() {
        anyhow::bail!(
            "VS found at {path} but vcvarsall.bat is missing. Install \"Desktop development with C++\".",
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
pub fn link_gcc(obj: &Path, exe: &Path) -> Result<()> {
    let exe_str = exe.display().to_string();
    let runtime_c = write_runtime_c(obj)?;

    if exe_str.ends_with(".exe") {
        for mingw in &["x86_64-w64-mingw32-gcc", "x86_64-w64-mingw32-ld"] {
            let mut cmd = Command::new(mingw);

            cmd.arg(obj)
                .arg("-o")
                .arg(exe)
                .arg("-lkernel32")
                .arg("-nostdlib")
                .arg("-e")
                .arg("main");

            if *mingw == "x86_64-w64-mingw32-gcc" {
                cmd.arg(&runtime_c);
            }

            let output = match cmd.output() {
                Ok(out) => out,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e).context(format!("spawning {mingw}")),
            };

            if output.status.success() {
                return Ok(());
            }

            eprintln!("{}", String::from_utf8_lossy(&output.stderr));

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
        .arg(&runtime_c)
        .output()
        .with_context(|| format!("{cc} link failed"))?;

    if !output.status.success() {
        let s = String::from_utf8_lossy(&output.stderr);

        eprintln!("{s}");

        anyhow::bail!("link failed");
    }

    Ok(())
}
