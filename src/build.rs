use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

fn project_name(entry: &Path) -> Option<String> {
    let root = crate::resolver::find_project_root(entry);
    let src = std::fs::read_to_string(root.join("init.toml")).ok()?;
    let value: toml::Table = src.parse().ok()?;

    value
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .map(String::from)
}

pub fn build(input: &Path, output: Option<&Path>) -> Result<PathBuf> {
    let input = if input.is_dir() {
        let main = input.join("main.fll");

        if !main.is_file() {
            anyhow::bail!("no `main.fll` found in {}", input.display());
        }

        main
    } else {
        input.to_path_buf()
    };

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
    let resolver = crate::resolver::ModuleResolver::resolve(&input)?;

    let obj_data = crate::codegen::compile(&resolver)
        .with_context(|| format!("failed to compile {}", input.display()))?;

    std::fs::write(&obj_path, &obj_data)
        .with_context(|| format!("write {}", obj_path.display()))?;

    let exe_path = output.map(|p| p.to_path_buf()).unwrap_or_else(|| {
        let name = if stem == "main" {
            project_name(&input).unwrap_or_else(|| "main".to_string())
        } else {
            stem.to_string()
        };

        let name = format!("{name}{exe_ext}");
        target.join(name)
    });

    #[cfg(target_os = "windows")]
    crate::link::link_win(&obj_path, &exe_path)?;

    #[cfg(not(target_os = "windows"))]
    crate::link::link_gcc(&obj_path, &exe_path)?;

    eprintln!("compiled to {}", exe_path.display());

    Ok(exe_path)
}
