use crate::syntax::ast::*;
use crate::syntax::lexer::Lexer;
use crate::syntax::parser::Parser;
use anyhow::{Context, Result, bail};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub type FuncMap = HashMap<String, Vec<(String, Vec<FllufType>)>>;

const SYSTEM_SRC: &str = include_str!("modules/System.fll");

pub struct ModuleResolver {
    pub all_functions: Vec<(String, Function)>,
    pub all_globals: Vec<(String, GlobalVar)>,
    pub func_map: FuncMap,
    pub global_map: HashMap<String, String>,
    pub return_types: HashMap<String, FllufType>,
}

struct ParsedModule {
    program: Program,
    dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
enum Export {
    Func {
        name: String,
        mangled: String,
        src_mp: Vec<String>,
        params: Vec<FllufType>,
    },
    Global {
        name: String,
        mangled: String,
        src_mp: Vec<String>,
    },
    Module {
        name: String,
        target: Vec<String>,
    },
}

impl Export {
    fn name(&self) -> &str {
        match self {
            Export::Func { name, .. }
            | Export::Global { name, .. }
            | Export::Module { name, .. } => name,
        }
    }
}

impl ModuleResolver {
    pub fn resolve(input: &Path) -> Result<Self> {
        let _project_root = find_project_root(input);
        let input_dir = input.parent().unwrap_or(Path::new("."));

        let entry_src =
            std::fs::read_to_string(input).with_context(|| format!("read {}", input.display()))?;

        let entry_prog = parse_src(&entry_src)?;

        let mut modules: HashMap<Vec<String>, ParsedModule> = HashMap::new();

        modules.insert(
            vec![],
            ParsedModule {
                program: entry_prog,
                dir: input_dir.to_path_buf(),
            },
        );

        modules.insert(
            vec!["System".to_string()],
            ParsedModule {
                program: parse_src(SYSTEM_SRC)?,
                dir: PathBuf::new(),
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
                .map(|pm| pm.program.uses.clone())
                .unwrap_or_default();

            for u in &uses {
                let path = match u {
                    Use::Module { path, .. }
                    | Use::Wildcard { path, .. }
                    | Use::Item { path, .. }
                    | Use::Items { path, .. } => path,
                };

                let base_dir = modules.get(&mp).unwrap().dir.clone();

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

                    let (found_path, _is_init) = if file_path.is_file() {
                        (file_path, false)
                    } else if init_path.is_file() {
                        (init_path, true)
                    } else {
                        bail!(
                            "module `{}` not found (tried {} and {})",
                            path[..=i].join("::"),
                            file_path.display(),
                            init_path.display()
                        );
                    };

                    let src = std::fs::read_to_string(&found_path)
                        .with_context(|| format!("read {}", found_path.display()))?;

                    let prog = parse_src(&src)?;
                    let dir = found_path.parent().unwrap().to_path_buf();

                    modules.insert(prefix.clone(), ParsedModule { program: prog, dir });

                    work.push(prefix.clone());
                }
            }
        }

        let mut module_exports: HashMap<Vec<String>, Vec<Export>> = HashMap::new();

        for (mp, pm) in &modules {
            let prefix = if mp.is_empty() {
                String::new()
            } else {
                let mut p = mp.join("__");

                p.push_str("__");

                p
            };

            let mut exports = Vec::new();

            let mut name_counts: HashMap<&str, usize> = HashMap::new();

            for f in &pm.program.functions {
                if f.pub_ || mp.is_empty() {
                    *name_counts.entry(f.name.as_str()).or_insert(0) += 1;
                }
            }

            for f in &pm.program.functions {
                if f.pub_ || mp.is_empty() {
                    let params: Vec<FllufType> =
                        f.params.iter().map(|p| p.param_type.clone()).collect();

                    let mangled = if mp.is_empty() && f.name == "Main" {
                        "main".to_string()
                    } else if name_counts.get(f.name.as_str()).copied().unwrap_or(0) > 1 {
                        let suffix: Vec<_> = params
                            .iter()
                            .map(|t| match t {
                                FllufType::Int => "int",
                                FllufType::Float => "float",
                                FllufType::String => "string",
                                FllufType::Void => "void",
                            })
                            .collect();

                        format!("{}{}__{}", prefix, f.name, suffix.join("_"))
                    } else {
                        format!("{}{}", prefix, f.name)
                    };

                    exports.push(Export::Func {
                        name: f.name.clone(),
                        mangled,
                        src_mp: mp.clone(),
                        params,
                    });
                }
            }

            for g in &pm.program.globals {
                if g.pub_ || mp.is_empty() {
                    let mangled = format!("{}{}", prefix, g.name);

                    exports.push(Export::Global {
                        name: g.name.clone(),
                        mangled,
                        src_mp: mp.clone(),
                    });
                }
            }

            module_exports.insert(mp.clone(), exports);
        }

        loop {
            let mut changed = false;
            let mod_keys: Vec<Vec<String>> = modules.keys().cloned().collect();

            for mp in &mod_keys {
                let pm = modules.get(mp).unwrap();
                let mut exports = module_exports.get(mp).unwrap().clone();

                for u in &pm.program.uses {
                    match u {
                        Use::Module { pub_: true, path } => {
                            let target = use_target(mp, path);

                            let name = path.last().unwrap().clone();

                            if modules.contains_key(&target)
                                && !exports.iter().any(|e| {
                                    matches!(e, Export::Module { name: n, target: t } if *n == name && *t == target)
                                })
                            {
                                exports.push(Export::Module { name, target });
                                changed = true;
                            }
                        }

                        Use::Wildcard { pub_: true, path } => {
                            let target = use_target(mp, path);

                            if let Some(target_exports) = module_exports.get(&target).cloned() {
                                for e in target_exports {
                                    let already = match &e {
                                        Export::Func { mangled, .. } => exports
                                            .iter()
                                            .any(|x| matches!(x, Export::Func { mangled: m, .. } if *m == *mangled)),
                                        Export::Global { mangled, .. } => exports.iter().any(
                                            |x| matches!(x, Export::Global { mangled: m, .. } if *m == *mangled),
                                        ),
                                        Export::Module { name, target } => exports.iter().any(|x| {
                                            matches!(x, Export::Module { name: n, target: t } if *n == *name && *t == *target)
                                        }),
                                    };

                                    if !already {
                                        exports.push(e);
                                        changed = true;
                                    }
                                }
                            }
                        }

                        Use::Item {
                            pub_: true,
                            path,
                            name,
                        } => {
                            let target = use_target(mp, path);

                            if let Some(e) = module_exports
                                .get(&target)
                                .and_then(|ex| ex.iter().find(|e| e.name() == name))
                                .cloned()
                            {
                                let already = match &e {
                                    Export::Func { mangled, .. } => exports
                                        .iter()
                                        .any(|x| matches!(x, Export::Func { mangled: m, .. } if *m == *mangled)),
                                    Export::Global { mangled, .. } => exports.iter().any(
                                        |x| matches!(x, Export::Global { mangled: m, .. } if *m == *mangled),
                                    ),
                                    Export::Module { name, target } => exports.iter().any(|x| {
                                        matches!(x, Export::Module { name: n, target: t } if *n == *name && *t == *target)
                                    }),
                                };

                                if !already {
                                    exports.push(e);
                                    changed = true;
                                }
                            }
                        }

                        Use::Items {
                            pub_: true,
                            path,
                            names,
                        } => {
                            let target = use_target(mp, path);

                            if let Some(target_exports) = module_exports.get(&target) {
                                for name in names {
                                    if let Some(e) =
                                        target_exports.iter().find(|e| e.name() == name).cloned()
                                    {
                                        let already = match &e {
                                            Export::Func { mangled, .. } => exports.iter().any(
                                                |x| matches!(x, Export::Func { mangled: m, .. } if *m == *mangled),
                                            ),
                                            Export::Global { mangled, .. } => exports.iter().any(
                                                |x| matches!(x, Export::Global { mangled: m, .. } if *m == *mangled),
                                            ),
                                            Export::Module { name, target } => exports.iter().any(
                                                |x| matches!(x, Export::Module { name: n, target: t } if *n == *name && *t == *target),
                                            ),
                                        };

                                        if !already {
                                            exports.push(e);
                                            changed = true;
                                        }
                                    }
                                }
                            }
                        }

                        _ => {}
                    }
                }

                module_exports.insert(mp.clone(), exports);
            }

            if !changed {
                break;
            }
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

            for e in exports {
                match e {
                    Export::Func {
                        name,
                        mangled,
                        src_mp,
                        params,
                    } => {
                        if func_seen.insert(mangled.clone()) {
                            let f = find_function(name, params, src_mp, &modules);
                            all_functions.push((mangled.clone(), f));
                        }

                        let key = if module_ns.is_empty() {
                            name.clone()
                        } else {
                            format!("{}::{}", module_ns, name)
                        };

                        insert_func(&mut func_map, key, mangled.clone(), params.clone());
                    }

                    Export::Global {
                        name,
                        mangled,
                        src_mp,
                    } => {
                        if global_seen.insert(mangled.clone()) {
                            let g = find_global(name, src_mp, &modules);
                            all_globals.push((mangled.clone(), g));
                        }

                        if !module_ns.is_empty() {
                            global_map.insert(format!("{}::{}", module_ns, name), mangled.clone());
                        }
                    }

                    Export::Module { .. } => {}
                }
            }
        }

        let mod_keys: Vec<Vec<String>> = modules.keys().cloned().collect();

        for mp in &mod_keys {
            let pm = modules.get(mp).unwrap();

            for u in &pm.program.uses {
                match u {
                    Use::Wildcard { path, .. } => {
                        let target = use_target(mp, path);

                        if let Some(target_exports) = module_exports.get(&target) {
                            let parent_ns = if target.len() <= 1 {
                                String::new()
                            } else {
                                target[..target.len() - 1].join("::")
                            };

                            for e in target_exports {
                                match e {
                                    Export::Func {
                                        name,
                                        mangled,
                                        params,
                                        ..
                                    } => {
                                        insert_func(
                                            &mut func_map,
                                            ns_key(&parent_ns, name),
                                            mangled.clone(),
                                            params.clone(),
                                        );
                                    }

                                    Export::Global { name, mangled, .. } => {
                                        global_map
                                            .insert(ns_key(&parent_ns, name), mangled.clone());
                                    }

                                    Export::Module { name, target: t } => {
                                        if let Some(member_exports) = module_exports.get(t) {
                                            for me in member_exports {
                                                match me {
                                                    Export::Func {
                                                        name: mn,
                                                        mangled,
                                                        params,
                                                        ..
                                                    } => {
                                                        insert_func(
                                                            &mut func_map,
                                                            ns_key(
                                                                &parent_ns,
                                                                &format!("{}::{}", name, mn),
                                                            ),
                                                            mangled.clone(),
                                                            params.clone(),
                                                        );
                                                    }

                                                    Export::Global {
                                                        name: mn, mangled, ..
                                                    } => {
                                                        global_map.insert(
                                                            ns_key(
                                                                &parent_ns,
                                                                &format!("{}::{}", name, mn),
                                                            ),
                                                            mangled.clone(),
                                                        );
                                                    }

                                                    Export::Module { .. } => {}
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    Use::Module { path, pub_ } if !pub_ || mp.is_empty() => {
                        let target = use_target(mp, path);

                        if let Some(target_exports) = module_exports.get(&target) {
                            for e in target_exports {
                                match e {
                                    Export::Func {
                                        name,
                                        mangled,
                                        params,
                                        ..
                                    } => {
                                        insert_func(
                                            &mut func_map,
                                            name.clone(),
                                            mangled.clone(),
                                            params.clone(),
                                        );
                                    }

                                    Export::Global { name, mangled, .. } => {
                                        global_map.insert(name.clone(), mangled.clone());
                                    }

                                    Export::Module { .. } => {}
                                }
                            }
                        }
                    }

                    Use::Item { path, name, pub_ } if !pub_ || mp.is_empty() => {
                        let target = use_target(mp, path);

                        if let Some(e) = module_exports
                            .get(&target)
                            .and_then(|ex| ex.iter().find(|e| e.name() == name))
                            .cloned()
                        {
                            match e {
                                Export::Func {
                                    mangled, params, ..
                                } => {
                                    insert_func(
                                        &mut func_map,
                                        name.clone(),
                                        mangled,
                                        params.clone(),
                                    );
                                }

                                Export::Global { mangled, .. } => {
                                    global_map.insert(name.clone(), mangled);
                                }

                                Export::Module { .. } => {}
                            }
                        }
                    }

                    Use::Items { path, names, pub_ } if !pub_ || mp.is_empty() => {
                        let target = use_target(mp, path);

                        if let Some(target_exports) = module_exports.get(&target) {
                            for name in names {
                                if let Some(e) =
                                    target_exports.iter().find(|e| e.name() == name).cloned()
                                {
                                    match e {
                                        Export::Func {
                                            mangled, params, ..
                                        } => {
                                            insert_func(
                                                &mut func_map,
                                                name.clone(),
                                                mangled,
                                                params.clone(),
                                            );
                                        }

                                        Export::Global { mangled, .. } => {
                                            global_map.insert(name.clone(), mangled);
                                        }

                                        Export::Module { .. } => {}
                                    }
                                }
                            }
                        }
                    }

                    _ => {}
                }
            }
        }

        let return_types = all_functions
            .iter()
            .map(|(mangled, f)| (mangled.clone(), f.return_type.clone()))
            .collect();

        Ok(ModuleResolver {
            all_functions,
            all_globals,
            func_map,
            global_map,
            return_types,
        })
    }
}

fn resolve_search_dir(module_dir: &Path, path: &[String]) -> PathBuf {
    let mut dir = module_dir.to_path_buf();

    for seg in &path[..path.len().saturating_sub(1)] {
        dir = dir.join(seg);
    }

    dir
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

fn parse_src(src: &str) -> Result<Program> {
    let toks: Vec<_> = Lexer::new(src).collect();
    Parser::new(toks).parse()
}

fn ns_key(parent_ns: &str, name: &str) -> String {
    if parent_ns.is_empty() {
        name.to_string()
    } else {
        format!("{}::{}", parent_ns, name)
    }
}

fn use_target(mp: &[String], path: &[String]) -> Vec<String> {
    if path.first().map(String::as_str) == Some("System") {
        path.to_vec()
    } else {
        let mut target = mp.to_vec();
        target.extend(path.iter().cloned());
        target
    }
}

fn insert_func(func_map: &mut FuncMap, key: String, mangled: String, params: Vec<FllufType>) {
    let e = func_map.entry(key).or_default();

    if !e.iter().any(|(m, _)| *m == mangled) {
        e.push((mangled, params));
    }
}

fn find_function(
    name: &str,
    params: &[FllufType],
    mod_path: &[String],
    modules: &HashMap<Vec<String>, ParsedModule>,
) -> Function {
    let pm = modules.get(mod_path).unwrap();

    for f in &pm.program.functions {
        if f.name == name
            && f.params.len() == params.len()
            && f.params.iter().zip(params).all(|(p, t)| &p.param_type == t)
        {
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
