use super::exports::Export;
use super::{ModuleFuncTable, ModuleGlobalTable, ParsedModule, ns_key, u_line, use_target};
use crate::error::Pos;
use crate::syntax::ast::{FllufType, Function, GlobalVar, Use};
use anyhow::Result;
use std::collections::{HashMap, HashSet};

pub(super) struct Qualified {
    funcs: ModuleFuncTable,
    globals: ModuleGlobalTable,
    root_funcs: Vec<(String, String, Vec<FllufType>)>,
}

type Functions = Vec<(String, Function, Vec<String>)>;
type Globals = Vec<(String, GlobalVar)>;
type ResolverTables = (
    HashMap<Vec<String>, ModuleFuncTable>,
    HashMap<Vec<String>, ModuleGlobalTable>,
);

pub(super) fn collect(
    modules: &HashMap<Vec<String>, ParsedModule>,
    module_exports: &HashMap<Vec<String>, Vec<Export>>,
) -> (Functions, Globals, Qualified) {
    let mut all_functions = Vec::new();
    let mut all_globals = Vec::new();
    let mut func_seen = HashSet::new();
    let mut global_seen = HashSet::new();

    let mut funcs: ModuleFuncTable = HashMap::new();
    let mut globals: ModuleGlobalTable = HashMap::new();
    let mut root_funcs: Vec<(String, String, Vec<FllufType>)> = Vec::new();

    for (mp, exports) in module_exports {
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
                        let f = find_function(name, params, src_mp, modules);
                        all_functions.push((mangled.clone(), f, src_mp.clone()));
                    }

                    if module_ns.is_empty() {
                        root_funcs.push((name.clone(), mangled.clone(), params.clone()));
                    } else {
                        insert_func(
                            &mut funcs,
                            format!("{}::{}", module_ns, name),
                            mangled.clone(),
                            params.clone(),
                        );
                    }
                }

                Export::Global {
                    name,
                    mangled,
                    src_mp,
                } => {
                    if global_seen.insert(mangled.clone()) {
                        let g = find_global(name, src_mp, modules);
                        all_globals.push((mangled.clone(), g));
                    }

                    if !module_ns.is_empty() {
                        globals.insert(format!("{}::{}", module_ns, name), mangled.clone());
                    }
                }

                Export::Module { .. } => {}
            }
        }
    }

    (
        all_functions,
        all_globals,
        Qualified {
            funcs,
            globals,
            root_funcs,
        },
    )
}

pub(super) fn build_maps(
    modules: &HashMap<Vec<String>, ParsedModule>,
    module_exports: &HashMap<Vec<String>, Vec<Export>>,
    qualified: &Qualified,
) -> Result<ResolverTables> {
    let mut func_map: HashMap<Vec<String>, ModuleFuncTable> = HashMap::new();
    let mut global_map: HashMap<Vec<String>, ModuleGlobalTable> = HashMap::new();

    let module_paths: Vec<Vec<String>> = modules.keys().cloned().collect();

    for mp in &module_paths {
        let mut ftable = qualified.funcs.clone();
        let mut gtable = qualified.globals.clone();

        for (name, mangled, params) in &qualified.root_funcs {
            insert_func(&mut ftable, name.clone(), mangled.clone(), params.clone());
        }

        let mut contributors = vec![mp.clone()];

        for i in 1..mp.len() {
            let anc: Vec<String> = mp[..i].to_vec();

            if modules
                .get(&anc)
                .is_some_and(|pm| pm.file.file_name().is_some_and(|n| n == "__init__.fll"))
            {
                contributors.push(anc);
            }
        }

        for c in &contributors {
            let pm = modules.get(c).unwrap();

            for u in &pm.program.uses {
                apply_use(c, u, module_exports, modules, &mut ftable, &mut gtable)?;
            }
        }

        func_map.insert(mp.clone(), ftable);
        global_map.insert(mp.clone(), gtable);
    }

    Ok((func_map, global_map))
}

fn apply_use(
    owner: &[String],
    u: &Use,
    module_exports: &HashMap<Vec<String>, Vec<Export>>,
    modules: &HashMap<Vec<String>, ParsedModule>,
    ftable: &mut ModuleFuncTable,
    gtable: &mut ModuleGlobalTable,
) -> Result<()> {
    let pm = modules.get(owner).unwrap();
    let line = u_line(u);

    match u {
        Use::Wildcard { path, .. } => {
            let target = use_target(owner, path);

            let target_exports = module_exports
                .get(&target)
                .ok_or_else(|| module_not_found(pm, line, &target))?;

            let parent_ns = if target.len() <= 1 {
                String::new()
            } else {
                target[..target.len() - 1].join("::")
            };

            for e in target_exports {
                match e {
                    Export::Module { name, target: t } => {
                        if let Some(member_exports) = module_exports.get(t) {
                            for me in member_exports {
                                add_export(
                                    ftable,
                                    gtable,
                                    &ns_key(&parent_ns, &format!("{name}::{}", me.name())),
                                    me,
                                );
                            }
                        }
                    }

                    e => add_export(ftable, gtable, &ns_key(&parent_ns, e.name()), e),
                }
            }
        }

        Use::Module { path, .. } => {
            let target = use_target(owner, path);

            let target_exports = module_exports
                .get(&target)
                .ok_or_else(|| module_not_found(pm, line, &target))?;

            for e in target_exports {
                add_export(ftable, gtable, e.name(), e);
            }
        }

        Use::Item { path, name, .. } => {
            let target = use_target(owner, path);

            let e = module_exports
                .get(&target)
                .and_then(|ex| ex.iter().find(|e| e.name() == name))
                .ok_or_else(|| not_exported(pm, line, name, &target))?;

            add_export(ftable, gtable, name, e);
        }

        Use::Items { path, names, .. } => {
            let target = use_target(owner, path);

            let target_exports = module_exports
                .get(&target)
                .ok_or_else(|| module_not_found(pm, line, &target))?;

            for name in names {
                let e = target_exports
                    .iter()
                    .find(|e| e.name() == name)
                    .ok_or_else(|| not_exported(pm, line, name, &target))?;

                add_export(ftable, gtable, name, e);
            }
        }
    }

    Ok(())
}

fn add_export(ftable: &mut ModuleFuncTable, gtable: &mut ModuleGlobalTable, key: &str, e: &Export) {
    match e {
        Export::Func {
            mangled, params, ..
        } => {
            insert_func(ftable, key.to_string(), mangled.clone(), params.clone());
        }

        Export::Global { mangled, .. } => {
            gtable.insert(key.to_string(), mangled.clone());
        }

        Export::Module { .. } => {}
    }
}

fn insert_func(
    func_map: &mut ModuleFuncTable,
    key: String,
    mangled: String,
    params: Vec<FllufType>,
) {
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

fn module_not_found(pm: &ParsedModule, line: usize, target: &[String]) -> anyhow::Error {
    anyhow::anyhow!(crate::error::render(
        &pm.file,
        Pos::new(line, 1),
        &format!("module `{}` not found", target.join("::")),
        &pm.lines
    ))
}

fn not_exported(pm: &ParsedModule, line: usize, name: &str, target: &[String]) -> anyhow::Error {
    anyhow::anyhow!(crate::error::render(
        &pm.file,
        Pos::new(line, 1),
        &format!("`{name}` is not exported by module `{}`", target.join("::")),
        &pm.lines
    ))
}
