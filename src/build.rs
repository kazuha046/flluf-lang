use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub fn build(input: &Path, output: Option<&Path>) -> Result<PathBuf> {
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

    let toks: Vec<crate::syntax::token::Token> =
        crate::syntax::lexer::Lexer::new(&src).collect();

    let program = crate::syntax::parser::Parser::new(toks)
        .parse()
        .with_context(|| format!("failed to parse {}", input.display()))?;

    let obj_data = crate::codegen::compile(&program)
        .with_context(|| format!("failed to compile {}", input.display()))?;

    std::fs::write(&obj_path, &obj_data)
        .with_context(|| format!("write {}", obj_path.display()))?;

    let exe_path = output.map(|p| p.to_path_buf()).unwrap_or_else(|| {
        let name = format!("{stem}{exe_ext}");
        target.join(name)
    });

    #[cfg(target_os = "windows")]
    crate::link::link_win(&obj_path, &exe_path)?;

    #[cfg(not(target_os = "windows"))]
    crate::link::link_gcc(&obj_path, &exe_path)?;

    eprintln!("compiled to {}", exe_path.display());

    Ok(exe_path)
}
