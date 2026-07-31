use crate::syntax::ast::*;
use crate::syntax::lexer::Lexer;
use crate::syntax::parser::Parser;
use anyhow::{Context, Result, bail};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub struct ModuleResolver {
    pub all_functions: Vec<(String, Function)>,
    pub all_globals: Vec<(String, GlobalVar)>,
    pub func_map: HashMap<String, String>,
    pub global_map: HashMap<String, String>,
    pub function_system: HashMap<String, bool>,
}

struct ParsedModule {
    program: Program,
    dir: PathBuf,
    is_init: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum Export {
    Func {
        name: String,
        mangled: String,
        src_mp: Vec<String>,
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
                is_init: false,
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
                    Use::System => continue,
                    Use::Module { path, .. }
                    | Use::Wildcard { path, .. }
                    | Use::Item { path, .. }
                    | Use::Items { path, .. } => path,
                };

                let base_dir = modules.get(&mp).unwrap().dir.clone();
                let mut prefix = mp.clone();

                for (i, seg) in path.iter().enumerate() {
                    prefix.push(seg.clone());

                    if modules.contains_key(&prefix) {
                        continue;
                    }

                    let search_dir = resolve_search_dir(&base_dir, &path[..=i]);
                    let file_path = search_dir.join(format!("{}.fll", seg));
                    let init_path = search_dir.join(seg).join("__init__.fll");

                    let (found_path, is_init) = if file_path.is_file() {
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

                    modules.insert(
                        prefix.clone(),
                        ParsedModule {
                            program: prog,
                            dir,
                            is_init,
                        },
                    );

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

            for f in &pm.program.functions {
                if f.pub_ || mp.is_empty() {
                    let mangled = if mp.is_empty() && f.name == "Main" {
                        "main".to_string()
                    } else {
                        format!("{}{}", prefix, f.name)
                    };

                    exports.push(Export::Func {
                        name: f.name.clone(),
                        mangled,
                        src_mp: mp.clone(),
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
                            let mut target = mp.clone();
                            target.extend(path.iter().cloned());

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
                            let mut target = mp.clone();
                            target.extend(path.iter().cloned());

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

                        Use::Item { pub_: true, path, name } => {
                            let mut target = mp.clone();
                            target.extend(path.iter().cloned());

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

                        Use::Items { pub_: true, path, names } => {
                            let mut target = mp.clone();
                            target.extend(path.iter().cloned());

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
                    } => {
                        if func_seen.insert(mangled.clone()) {
                            let f = find_function(name, src_mp, &modules);
                            all_functions.push((mangled.clone(), f));
                        }

                        if !module_ns.is_empty() {
                            func_map.insert(format!("{}::{}", module_ns, name), mangled.clone());
                        }
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
                        let mut target = mp.clone();
                        target.extend(path.iter().cloned());

                        if let Some(target_exports) = module_exports.get(&target) {
                            let parent_ns = if target.len() <= 1 {
                                String::new()
                            } else {
                                target[..target.len() - 1].join("::")
                            };

                            for e in target_exports {
                                match e {
                                    Export::Func { name, mangled, .. } => {
                                        func_map.insert(ns_key(&parent_ns, name), mangled.clone());
                                    }

                                    Export::Global { name, mangled, .. } => {
                                        global_map.insert(ns_key(&parent_ns, name), mangled.clone());
                                    }

                                    Export::Module { name, target: t } => {
                                        if let Some(member_exports) = module_exports.get(t) {
                                            for me in member_exports {
                                                match me {
                                                    Export::Func { name: mn, mangled, .. } => {
                                                        func_map.insert(
                                                            ns_key(&parent_ns, &format!("{}::{}", name, mn)),
                                                            mangled.clone(),
                                                        );
                                                    }

                                                    Export::Global { name: mn, mangled, .. } => {
                                                        global_map.insert(
                                                            ns_key(&parent_ns, &format!("{}::{}", name, mn)),
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

                    Use::Item { path, name, pub_ } if !pub_ || mp.is_empty() => {
                        let mut target = mp.clone();
                        target.extend(path.iter().cloned());

                        if let Some(e) = module_exports
                            .get(&target)
                            .and_then(|ex| ex.iter().find(|e| e.name() == name))
                            .cloned()
                        {
                            match e {
                                Export::Func { mangled, .. } => {
                                    func_map.insert(name.clone(), mangled);
                                }

                                Export::Global { mangled, .. } => {
                                    global_map.insert(name.clone(), mangled);
                                }

                                Export::Module { .. } => {}
                            }
                        }
                    }

                    Use::Items { path, names, pub_ } if !pub_ || mp.is_empty() => {
                        let mut target = mp.clone();
                        target.extend(path.iter().cloned());

                        if let Some(target_exports) = module_exports.get(&target) {
                            for name in names {
                                if let Some(e) =
                                    target_exports.iter().find(|e| e.name() == name).cloned()
                                {
                                    match e {
                                        Export::Func { mangled, .. } => {
                                            func_map.insert(name.clone(), mangled);
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

        let sys_map = compute_sys_map(&modules);
        let mut function_system = HashMap::new();

        for exports in module_exports.values() {
            for e in exports {
                if let Export::Func {
                    mangled,
                    src_mp,
                    ..
                } = e
                {
                    let has = sys_map.get(src_mp).copied().unwrap_or(false);
                    function_system.insert(mangled.clone(), has);
                }
            }
        }

        Ok(ModuleResolver {
            all_functions,
            all_globals,
            func_map,
            global_map,
            function_system,
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

fn ns_key(parent_ns: &str, name: &str) -> String {
    if parent_ns.is_empty() {
        name.to_string()
    } else {
        format!("{}::{}", parent_ns, name)
    }
}

fn compute_sys_map(modules: &HashMap<Vec<String>, ParsedModule>) -> HashMap<Vec<String>, bool> {
    let mut memo = HashMap::new();

    let keys: Vec<Vec<String>> = modules.keys().cloned().collect();

    for mp in &keys {
        compute_sys(mp, modules, &mut memo);
    }

    memo
}

fn compute_sys(
    mp: &[String],
    modules: &HashMap<Vec<String>, ParsedModule>,
    memo: &mut HashMap<Vec<String>, bool>,
) -> bool {
    if let Some(v) = memo.get(mp) {
        return *v;
    }

    let pm = modules.get(mp).unwrap();
    let mut v = pm.program.use_system;

    if !pm.is_init && mp.len() > 1 {
        let pkg = mp[..mp.len() - 1].to_vec();

        if modules.get(&pkg).is_some_and(|p| p.is_init) {
            v = v || compute_sys(&pkg, modules, memo);
        }
    }    memo.insert(mp.to_vec(), v);

    v
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
