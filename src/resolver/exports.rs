use super::{ParsedModule, use_target};
use crate::syntax::ast::{FllufType, Use};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Export {
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
    pub(super) fn name(&self) -> &str {
        match self {
            Export::Func { name, .. }
            | Export::Global { name, .. }
            | Export::Module { name, .. } => name,
        }
    }
}

pub(super) fn build_exports(
    modules: &HashMap<Vec<String>, ParsedModule>,
) -> HashMap<Vec<String>, Vec<Export>> {
    let mut module_exports = initial_exports(modules);

    loop {
        let mut changed = false;
        let mod_keys: Vec<Vec<String>> = modules.keys().cloned().collect();

        for mp in &mod_keys {
            let pm = modules.get(mp).unwrap();
            let mut exports = module_exports.get(mp).unwrap().clone();

            for u in &pm.program.uses {
                let (pub_, path) = match u {
                    Use::Module { pub_, path, .. }
                    | Use::Wildcard { pub_, path, .. }
                    | Use::Item { pub_, path, .. }
                    | Use::Items { pub_, path, .. } => (pub_, path),
                };

                if !pub_ {
                    continue;
                }

                let target = use_target(mp, path);

                let Some(target_exports) = module_exports.get(&target) else {
                    continue;
                };

                for e in exported_from(target_exports, u, mp, modules) {
                    if !exports_contain(&exports, &e) {
                        exports.push(e);
                        changed = true;
                    }
                }
            }

            module_exports.insert(mp.clone(), exports);
        }

        if !changed {
            break;
        }
    }

    module_exports
}

fn initial_exports(
    modules: &HashMap<Vec<String>, ParsedModule>,
) -> HashMap<Vec<String>, Vec<Export>> {
    let mut module_exports: HashMap<Vec<String>, Vec<Export>> = HashMap::new();

    for (mp, pm) in modules {
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
                    let suffix: Vec<_> = params.iter().map(type_suffix).collect();

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
                exports.push(Export::Global {
                    name: g.name.clone(),
                    mangled: format!("{}{}", prefix, g.name),
                    src_mp: mp.clone(),
                });
            }
        }

        module_exports.insert(mp.clone(), exports);
    }

    module_exports
}

fn exported_from(
    target_exports: &[Export],
    u: &Use,
    mp: &[String],
    modules: &HashMap<Vec<String>, ParsedModule>,
) -> Vec<Export> {
    match u {
        Use::Module { path, .. } => {
            let target = use_target(mp, path);

            if modules.contains_key(&target) {
                vec![Export::Module {
                    name: path.last().unwrap().clone(),
                    target,
                }]
            } else {
                Vec::new()
            }
        }

        Use::Wildcard { .. } => target_exports.to_vec(),

        Use::Item { name, .. } => target_exports
            .iter()
            .filter(|e| e.name() == name)
            .cloned()
            .collect(),

        Use::Items { names, .. } => {
            let mut out = Vec::new();

            for n in names {
                out.extend(target_exports.iter().filter(|e| e.name() == n).cloned());
            }

            out
        }
    }
}

fn exports_contain(exports: &[Export], e: &Export) -> bool {
    match e {
        Export::Func { mangled, .. } => exports
            .iter()
            .any(|x| matches!(x, Export::Func { mangled: m, .. } if *m == *mangled)),

        Export::Global { mangled, .. } => exports
            .iter()
            .any(|x| matches!(x, Export::Global { mangled: m, .. } if *m == *mangled)),

        Export::Module { name, target } => exports.iter().any(
            |x| matches!(x, Export::Module { name: n, target: t } if *n == *name && *t == *target),
        ),
    }
}

fn type_suffix(t: &FllufType) -> &'static str {
    match t {
        FllufType::Int => "int",
        FllufType::Float => "float",
        FllufType::String => "string",
        FllufType::Void => "void",
    }
}
