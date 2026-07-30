use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

#[cfg(target_os = "windows")]
use std::path::PathBuf;

#[cfg(target_os = "windows")]
pub fn link_win(obj: &Path, exe: &Path) -> Result<()> {
    let obj_str = obj.display().to_string();
    let exe_str = exe.display().to_string();

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

        anyhow::bail!("link.exe failed:\n{}", String::from_utf8_lossy(&out.stderr));
    }

    for mingw in &["x86_64-w64-mingw32-gcc", "x86_64-w64-mingw32-ld"] {
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
                .context(format!("running {mingw}"))?;

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

    let vcvars = vs_path.join(r"VC\Auxiliary\Build\vcvarsall.bat");

    let script = format!(
        "@echo off\r\n\
         call \"{}\" x64\r\n\
         if errorlevel 1 exit /b 1\r\n\
         link.exe /NOLOGO /OUT:\"{exe_str}\" /SUBSYSTEM:CONSOLE /NODEFAULTLIB /ENTRY:main \"{obj_str}\" kernel32.lib\r\n\
         exit /b %errorlevel%\r\n",
        vcvars.display()
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
        .output()
        .with_context(|| format!("{cc} link failed"))?;

    if !output.status.success() {
        let s = String::from_utf8_lossy(&output.stderr);

        eprintln!("{s}");

        anyhow::bail!("link failed");
    }

    Ok(())
}
