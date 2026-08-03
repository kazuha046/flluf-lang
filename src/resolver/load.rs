use super::{ParsedModule, u_line};
use crate::syntax::ast::{Program, Use};
use crate::syntax::lexer::Lexer;
use crate::syntax::parser::Parser;
use anyhow::{Context, Result, bail};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

const SYSTEM_SRC: &str = include_str!("../modules/System.fll");

pub(super) fn load_modules(input: &Path) -> Result<HashMap<Vec<String>, ParsedModule>> {
    let _project_root = find_project_root(input);
    let input_dir = input.parent().unwrap_or(Path::new("."));

    let entry_src =
        std::fs::read_to_string(input).with_context(|| format!("read {}", input.display()))?;

    let mut modules: HashMap<Vec<String>, ParsedModule> = HashMap::new();

    modules.insert(
        vec![],
        parsed_module(
            parse_src(&entry_src, input)?,
            input_dir.to_path_buf(),
            input.to_path_buf(),
            &entry_src,
        ),
    );

    modules.insert(
        vec!["System".to_string()],
        parsed_module(
            parse_src(SYSTEM_SRC, Path::new("System"))?,
            PathBuf::new(),
            PathBuf::from("System"),
            SYSTEM_SRC,
        ),
    );

    let mut work: Vec<Vec<String>> = vec![vec![]];
    let mut visited: HashSet<Vec<String>> = HashSet::new();

    while let Some(mp) = work.pop() {
        if !visited.insert(mp.clone()) {
            continue;
        }

        let uses: Vec<Use> = modules
            .get(&mp)
            .map(|pm| pm.program.uses.clone())
            .unwrap_or_default();

        for u in &uses {
            let path = match u {
                Use::Module { path, .. }
                | Use::Wildcard { path, .. }
                | Use::Item { path, .. }
                | Use::Items { path, .. } => path,
            };

            let pm = modules.get(&mp).unwrap();
            let base_dir = pm.dir.clone();
            let ufile = pm.file.clone();
            let ulines = pm.lines.clone();
            let line = u_line(u);

            let mut prefix = if path.first().map(String::as_str) == Some("System") {
                Vec::new()
            } else {
                mp.clone()
            };

            for (i, seg) in path.iter().enumerate() {
                prefix.push(seg.clone());

                if modules.contains_key(&prefix) {
                    continue;
                }

                let search_dir = resolve_search_dir(&base_dir, &path[..=i]);
                let file_path = search_dir.join(format!("{}.fll", seg));
                let init_path = search_dir.join(seg).join("__init__.fll");

                let found_path = if file_path.is_file() {
                    file_path
                } else if init_path.is_file() {
                    init_path
                } else {
                    bail!(
                        "{}",
                        crate::error::render(
                            &ufile,
                            crate::error::Pos::new(line, 1),
                            &format!(
                                "module `{}` not found (tried {} and {})",
                                path[..=i].join("::"),
                                file_path.display(),
                                init_path.display()
                            ),
                            &ulines
                        )
                    );
                };

                let src = std::fs::read_to_string(&found_path)
                    .with_context(|| format!("read {}", found_path.display()))?;

                let dir = found_path.parent().unwrap().to_path_buf();

                modules.insert(
                    prefix.clone(),
                    parsed_module(parse_src(&src, &found_path)?, dir, found_path, &src),
                );

                work.push(prefix.clone());
            }
        }
    }

    Ok(modules)
}

pub(crate) fn find_project_root(input: &Path) -> PathBuf {
    let mut dir = input
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    loop {
        if dir.join("init.toml").exists() {
            return dir;
        }

        if !dir.pop() {
            return input
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."));
        }
    }
}

fn resolve_search_dir(module_dir: &Path, path: &[String]) -> PathBuf {
    let mut dir = module_dir.to_path_buf();

    for seg in &path[..path.len().saturating_sub(1)] {
        dir = dir.join(seg);
    }

    dir
}

fn parsed_module(program: Program, dir: PathBuf, file: PathBuf, src: &str) -> ParsedModule {
    ParsedModule {
        program,
        dir,
        file,
        lines: src.lines().map(String::from).collect(),
    }
}

fn parse_src(src: &str, file: &Path) -> Result<Program> {
    let mut lexer = Lexer::new(src);
    let toks: Vec<_> = lexer.by_ref().collect();

    if let Some((pos, msg)) = lexer.error() {
        let lines: Vec<String> = src.lines().map(String::from).collect();

        return Err(anyhow::anyhow!(crate::error::render(
            file, *pos, msg, &lines
        )));
    }

    let lines: Vec<String> = src.lines().map(String::from).collect();

    Parser::new(toks, file.to_path_buf(), lines).parse()
}
