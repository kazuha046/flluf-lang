mod exports;
mod load;
mod scope;

pub(crate) use load::find_project_root;

use crate::error::Pos;
use crate::syntax::ast::*;
use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub type ModuleFuncTable = HashMap<String, Vec<(String, Vec<FllufType>)>>;
pub type ModuleGlobalTable = HashMap<String, String>;

pub struct ModuleResolver {
    pub all_functions: Vec<(String, Function, Vec<String>)>,
    pub all_globals: Vec<(String, GlobalVar)>,
    pub func_map: HashMap<Vec<String>, ModuleFuncTable>,
    pub global_map: HashMap<Vec<String>, ModuleGlobalTable>,
    pub return_types: HashMap<String, FllufType>,
    pub sources: HashMap<Vec<String>, (PathBuf, Vec<String>)>,
}

struct ParsedModule {
    program: Program,
    dir: PathBuf,
    file: PathBuf,
    lines: Vec<String>,
}

impl ModuleResolver {
    pub fn resolve(input: &Path) -> Result<Self> {
        let modules = load::load_modules(input)?;
        let module_exports = exports::build_exports(&modules);

        let (all_functions, all_globals, qualified) = scope::collect(&modules, &module_exports);
        let (func_map, global_map) = scope::build_maps(&modules, &module_exports, &qualified)?;

        let return_types = all_functions
            .iter()
            .map(|(mangled, f, _)| (mangled.clone(), f.return_type.clone()))
            .collect();

        let sources = modules
            .iter()
            .map(|(mp, pm)| (mp.clone(), (pm.file.clone(), pm.lines.clone())))
            .collect();

        Ok(ModuleResolver {
            all_functions,
            all_globals,
            func_map,
            global_map,
            return_types,
            sources,
        })
    }

    pub fn render(&self, mp: &[String], pos: Pos, msg: &str) -> anyhow::Error {
        match self.sources.get(mp) {
            Some((file, lines)) => anyhow::anyhow!(crate::error::render(file, pos, msg, lines)),
            None => anyhow::anyhow!("{msg} (at line {})", pos.line),
        }
    }
}

pub(crate) fn u_line(u: &Use) -> usize {
    match u {
        Use::Module { line, .. }
        | Use::Wildcard { line, .. }
        | Use::Item { line, .. }
        | Use::Items { line, .. } => *line,
    }
}

pub(crate) fn ns_key(parent_ns: &str, name: &str) -> String {
    if parent_ns.is_empty() {
        name.to_string()
    } else {
        format!("{}::{}", parent_ns, name)
    }
}

pub(crate) fn use_target(mp: &[String], path: &[String]) -> Vec<String> {
    if path.first().map(String::as_str) == Some("System") {
        path.to_vec()
    } else {
        let mut target = mp.to_vec();

        target.extend(path.iter().cloned());

        target
    }
}
