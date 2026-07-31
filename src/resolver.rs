use crate::syntax::ast::*;
use crate::syntax::lexer::Lexer;
use crate::syntax::parser::Parser;
use anyhow::{Context, Result, bail};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub struct ModuleResolver {
    pub all_functions: Vec<(String, Function)>,
    pub all_globals: Vec<(String, GlobalVar)>,
    pub use_system: bool,
    pub func_map: HashMap<String, String>,
    pub global_map: HashMap<String, String>,
}

struct ParsedModule {
    program: Program,
    dir: PathBuf,
}

impl ModuleResolver {
    pub fn resolve(input: &Path) -> Result<Self> {
        let _project_root = find_project_root(input);
        let input_dir = input.parent().unwrap_or(Path::new("."));

        let entry_src =
            std::fs::read_to_string(input).with_context(|| format!("read {}", input.display()))?;

        let entry_prog = parse_src(&entry_src)?;

        let mut modules: HashMap<Vec<String>, ParsedModule> = HashMap::new();
        let mut use_system = entry_prog.use_system;

        modules.insert(
            vec![],
            ParsedModule {
                program: entry_prog,
                dir: input_dir.to_path_buf(),
            },
        );

        let mut work: Vec<Vec<String>> = vec![vec![]];
        let mut visited: HashSet<Vec<String>> = HashSet::new();

        while let Some(mp) = work.pop() {
            if !visited.insert(mp.clone()) {
                continue;
            }

            let uses: Vec<Use> = modules
                .get(&mp)
                .map(|pm| {
                    use_system = use_system || pm.program.use_system;
                    pm.program.uses.clone()
                })
                .unwrap_or_default();

            for u in &uses {
                match u {
                    Use::System => {}
                    Use::Module(path) | Use::Wildcard(path) => {
                        let dir = modules.get(&mp).unwrap().dir.clone();
                        let search_dir = resolve_search_dir(&dir, path);
                        let child_name = path.last().unwrap();
                        let file_path = search_dir.join(format!("{}.fll", child_name));
                        let init_path = search_dir.join(child_name).join("__init__.fll");

                        let child_mod_path: Vec<String> =
                            mp.iter().cloned().chain(path.iter().cloned()).collect();

                        if modules.contains_key(&child_mod_path) {
                            continue;
                        }

                        let (found_path, _is_init) = if file_path.is_file() {
                            (file_path, false)
                        } else if init_path.is_file() {
                            (init_path, true)
                        } else {
                            bail!(
                                "module `{}` not found (tried {} and {})",
                                path.join("::"),
                                file_path.display(),
                                init_path.display()
                            );
                        };

                        let src = std::fs::read_to_string(&found_path)
                            .with_context(|| format!("read {}", found_path.display()))?;

                        let prog = parse_src(&src)?;
                        let dir = found_path.parent().unwrap().to_path_buf();

                        modules.insert(child_mod_path.clone(), ParsedModule { program: prog, dir });
                        work.push(child_mod_path);
                    }
                }
            }
        }

        let mut module_exports: HashMap<Vec<String>, Vec<(String, String, bool, Vec<String>)>> =
            HashMap::new();

        for (mp, pm) in &modules {
            let prefix = if mp.is_empty() {
                String::new()
            } else {
                let mut p = mp.join("__");

                p.push_str("__");

                p
            };

            let mut exports = Vec::new();

            for f in &pm.program.functions {
                if f.pub_ || mp.is_empty() {
                    let mangled = if mp.is_empty() && f.name == "Main" {
                        "main".to_string()
                    } else {
                        format!("{}{}", prefix, f.name)
                    };

                    exports.push((f.name.clone(), mangled, true, mp.clone()));
                }
            }

            for g in &pm.program.globals {
                if g.pub_ || mp.is_empty() {
                    let mangled = format!("{}{}", prefix, g.name);
                    exports.push((g.name.clone(), mangled, false, mp.clone()));
                }
            }

            module_exports.insert(mp.clone(), exports);
        }

        let mod_keys: Vec<Vec<String>> = modules.keys().cloned().collect();

        for mp in &mod_keys {
            let pm = modules.get(mp).unwrap();
            let mut exports = module_exports.get(mp).unwrap().clone();

            for u in &pm.program.uses {
                if let Use::Wildcard(path) = u {
                    let mut child_path = mp.clone();

                    child_path.extend(path.iter().cloned());

                    if let Some(child_exports) = module_exports.get(&child_path) {
                        for (name, mangled, is_func, src_mp) in child_exports {
                            exports.push((name.clone(), mangled.clone(), *is_func, src_mp.clone()));
                        }
                    }
                }
            }

            module_exports.insert(mp.clone(), exports);
        }

        let mut all_functions = Vec::new();
        let mut all_globals = Vec::new();
        let mut func_map = HashMap::new();
        let mut global_map = HashMap::new();
        let mut func_seen = HashSet::new();
        let mut global_seen = HashSet::new();

        for (mp, exports) in &module_exports {
            let module_ns = if mp.is_empty() {
                String::new()
            } else {
                mp.join("::")
            };

            for (name, mangled, is_func, src_mp) in exports {
                if *is_func {
                    if func_seen.insert(mangled.clone()) {
                        let f = find_function(name, src_mp, &modules);
                        all_functions.push((mangled.clone(), f));
                    }

                    if !module_ns.is_empty() {
                        func_map.insert(format!("{}::{}", module_ns, name), mangled.clone());
                    }
                } else {
                    if global_seen.insert(mangled.clone()) {
                        let g = find_global(name, src_mp, &modules);
                        all_globals.push((mangled.clone(), g));
                    }

                    if !module_ns.is_empty() {
                        global_map.insert(format!("{}::{}", module_ns, name), mangled.clone());
                    }
                }
            }
        }

        Ok(ModuleResolver {
            all_functions,
            all_globals,
            use_system,
            func_map,
            global_map,
        })
    }
}

fn resolve_search_dir(module_dir: &Path, path: &[String]) -> PathBuf {
    let mut dir = module_dir.to_path_buf();

    for i in 0..path.len().saturating_sub(1) {
        dir = dir.join(&path[i]);
    }

    dir
}

fn find_project_root(input: &Path) -> PathBuf {
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

fn parse_src(src: &str) -> Result<Program> {
    let toks: Vec<_> = Lexer::new(src).collect();
    Parser::new(toks).parse()
}

fn find_function(
    name: &str,
    mod_path: &[String],
    modules: &HashMap<Vec<String>, ParsedModule>,
) -> Function {
    let pm = modules.get(mod_path).unwrap();

    for f in &pm.program.functions {
        if f.name == name {
            return f.clone();
        }
    }

    panic!("function {name} not found in module {:?}", mod_path);
}

fn find_global(
    name: &str,
    mod_path: &[String],
    modules: &HashMap<Vec<String>, ParsedModule>,
) -> GlobalVar {
    let pm = modules.get(mod_path).unwrap();

    for g in &pm.program.globals {
        if g.name == name {
            return g.clone();
        }
    }

    panic!("global {name} not found in module {:?}", mod_path);
}
