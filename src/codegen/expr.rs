use crate::codegen::*;
use crate::resolver::ModuleResolver;
use crate::syntax::ast::{BinOp, Expr, FllufType};
use anyhow::{Result, bail};
use cranelift::codegen::ir::types;
use cranelift::prelude::*;
use cranelift_frontend::FunctionBuilder;
use cranelift_module::Module;

impl Compiler {
    pub fn compile_expr(
        &mut self,
        b: &mut FunctionBuilder,
        vars: &mut VarMap,
        e: &Expr,
        resolver: &ModuleResolver,
    ) -> Result<TypedValue> {
        match e {
            Expr::IntLit(n) => Ok(TypedValue {
                val: b.ins().iconst(types::I64, *n),
                tag: Tag::Int,
            }),

            Expr::FloatLit(n) => {
                let bits = n.to_bits() as i64;
                let tmp = b.ins().iconst(types::I64, bits);
                let val = b.ins().bitcast(types::F64, MemFlagsData::new(), tmp);

                Ok(TypedValue {
                    val,
                    tag: Tag::Float,
                })
            }

            Expr::StringLit(s) => Ok(TypedValue {
                val: self.store_string(b, s),
                tag: Tag::Ptr,
            }),

            Expr::Variable(name) => {
                let &(slot, tag, _) = vars
                    .get(name.as_str())
                    .ok_or_else(|| anyhow::anyhow!("undefined variable `{name}`"))?;

                let ir_ty = type_to_ir_tag(tag);
                let val = b.ins().stack_load(types::I64, ir_ty, slot, 0);

                Ok(TypedValue { val, tag })
            }

            Expr::BinaryOp(l, op, r) => self.compile_binop(b, vars, l, *op, r, resolver),

            Expr::Call(name, args) if name == "Log" => {
                self.compile_log(b, vars, name, args, resolver)
            }

            Expr::Call(name, args) => self.compile_call(b, vars, name, args, resolver),

            Expr::ModuleCall(mod_name, func_name, args) => {
                let key = format!("{}::{}", mod_name, func_name);

                if mod_name == "System" && func_name == "Log" {
                    self.compile_log(b, vars, &key, args, resolver)
                } else if !resolver.func_map.contains_key(&key) {
                    bail!("unknown function `{key}`");
                } else {
                    self.compile_call(b, vars, &key, args, resolver)
                }
            }

            Expr::ModuleVar(mod_name, var_name) => {
                let key = format!("{}::{}", mod_name, var_name);

                let mangled = resolver
                    .global_map
                    .get(&key)
                    .ok_or_else(|| anyhow::anyhow!("unknown global `{key}`"))?;

                let ptr = self.load_global(b, mangled);

                Ok(TypedValue {
                    val: ptr,
                    tag: Tag::Ptr,
                })
            }
        }
    }

    pub fn compile_binop(
        &mut self,
        b: &mut FunctionBuilder,
        vars: &mut VarMap,
        left: &Expr,
        op: BinOp,
        right: &Expr,
        resolver: &ModuleResolver,
    ) -> Result<TypedValue> {
        let l = self.compile_expr(b, vars, left, resolver)?;
        let r = self.compile_expr(b, vars, right, resolver)?;

        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
                let is_float = l.tag == Tag::Float || r.tag == Tag::Float;

                if is_float {
                    self.check_types(&l, &r)?;

                    let lv = widen(b, l.val);
                    let rv = widen(b, r.val);

                    let val = match op {
                        BinOp::Add => b.ins().fadd(lv, rv),
                        BinOp::Sub => b.ins().fsub(lv, rv),
                        BinOp::Mul => b.ins().fmul(lv, rv),
                        BinOp::Div => b.ins().fdiv(lv, rv),

                        _ => unreachable!(),
                    };
                    Ok(TypedValue {
                        val,
                        tag: Tag::Float,
                    })
                } else {
                    self.check_types(&l, &r)?;

                    let val = match op {
                        BinOp::Add => b.ins().iadd(l.val, r.val),
                        BinOp::Sub => b.ins().isub(l.val, r.val),
                        BinOp::Mul => b.ins().imul(l.val, r.val),
                        BinOp::Div => b.ins().sdiv(l.val, r.val),

                        _ => unreachable!(),
                    };

                    Ok(TypedValue { val, tag: Tag::Int })
                }
            }

            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
                let is_float = l.tag == Tag::Float || r.tag == Tag::Float;

                if is_float {
                    self.check_types(&l, &r)?;

                    let lv = widen(b, l.val);
                    let rv = widen(b, r.val);
                    let cmp = b.ins().fcmp(float_cc(op), lv, rv);

                    Ok(select_int(b, cmp))
                } else {
                    self.check_types(&l, &r)?;

                    let cmp = b.ins().icmp(int_cc(op), l.val, r.val);

                    Ok(select_int(b, cmp))
                }
            }
        }
    }

    pub fn compile_call(
        &mut self,
        b: &mut FunctionBuilder,
        vars: &mut VarMap,
        name: &str,
        args: &[Expr],
        resolver: &ModuleResolver,
    ) -> Result<TypedValue> {
        let (tv, _) = self.compile_call_inner(b, vars, name, args, resolver)?;

        Ok(tv)
    }

    fn compile_log(
        &mut self,
        b: &mut FunctionBuilder,
        vars: &mut VarMap,
        name: &str,
        args: &[Expr],
        resolver: &ModuleResolver,
    ) -> Result<TypedValue> {
        let (_, compiled) = self.compile_call_inner(b, vars, name, args, resolver)?;

        Ok(compiled.into_iter().next().unwrap_or(TypedValue {
            val: b.ins().iconst(types::I64, 0),
            tag: Tag::Int,
        }))
    }

    fn compile_call_inner(
        &mut self,
        b: &mut FunctionBuilder,
        vars: &mut VarMap,
        name: &str,
        args: &[Expr],
        resolver: &ModuleResolver,
    ) -> Result<(TypedValue, Vec<TypedValue>)> {
        let mut compiled = Vec::with_capacity(args.len());
        let mut tags = Vec::with_capacity(args.len());

        for a in args {
            let tv = self.compile_expr(b, vars, a, resolver)?;
            tags.push(tv.tag);
            compiled.push(tv);
        }

        let target = if let Some(fod) = self.module.get_name(name) {
            match fod {
                cranelift_module::FuncOrDataId::Func(id) => {
                    let decl = self.module.declarations().get_function_decl(id);

                    let params: Vec<_> =
                        decl.signature.params.iter().map(|p| p.value_type).collect();

                    (id, params, resolver.return_types.get(name).cloned())
                }

                _ => bail!("`{name}` is not a function"),
            }
        } else if let Some(candidates) = resolver.func_map.get(name) {
            let (id, params, ret_ty) = if candidates.len() == 1 {
                let (mangled, _) = &candidates[0];

                let id = match self.module.get_name(mangled) {
                    Some(cranelift_module::FuncOrDataId::Func(id)) => id,
                    _ => bail!("`{name}` resolves to an unknown function"),
                };

                let decl = self.module.declarations().get_function_decl(id);

                (
                    id,
                    decl.signature.params.iter().map(|p| p.value_type).collect(),
                    resolver.return_types.get(mangled).cloned(),
                )
            } else {
                let matches: Vec<_> = candidates
                    .iter()
                    .filter(|(_, params)| {
                        params.len() == tags.len()
                            && params.iter().zip(&tags).all(|(t, tag)| {
                                let want = match tag {
                                    Tag::Int => FllufType::Int,
                                    Tag::Float => FllufType::Float,
                                    Tag::Ptr => FllufType::String,
                                };

                                t == &want
                            })
                    })
                    .collect();

                match matches.len() {
                    0 => bail!("no overload of `{name}` matches the arguments"),
                    1 => {
                        let (mangled, _) = matches[0];

                        let id = match self.module.get_name(mangled) {
                            Some(cranelift_module::FuncOrDataId::Func(id)) => id,
                            _ => bail!("`{name}` resolves to an unknown function"),
                        };

                        let decl = self.module.declarations().get_function_decl(id);

                        (
                            id,
                            decl.signature.params.iter().map(|p| p.value_type).collect(),
                            resolver.return_types.get(mangled).cloned(),
                        )
                    }

                    _ => bail!("ambiguous call to `{name}`"),
                }
            };

            (id, params, ret_ty)
        } else if resolver.func_map.contains_key(&format!("System::{name}")) {
            bail!("'{name}' requires 'use System;'");
        } else {
            let mut sig = self.module.make_signature();

            for tv in &compiled {
                sig.params
                    .push(AbiParam::new(b.func.dfg.value_type(tv.val)));
            }

            sig.returns.push(AbiParam::new(types::I64));

            let id = self.import(name, &sig);
            let params: Vec<_> = sig.params.iter().map(|p| p.value_type).collect();

            (id, params, None)
        };

        let (id, expected, ret_ty) = target;

        let mut call_args = Vec::with_capacity(compiled.len());

        for (i, tv) in compiled.iter().enumerate() {
            let val_ty = b.func.dfg.value_type(tv.val);
            let exp = expected.get(i).copied().unwrap_or(val_ty);

            call_args.push(match (val_ty, exp) {
                (types::I64, types::F64) => b.ins().fcvt_from_sint(types::F64, tv.val),
                (types::F64, types::I64) => b.ins().fcvt_to_sint(types::I64, tv.val),
                _ => tv.val,
            });
        }

        let func_ref = self.module.declare_func_in_func(id, b.func);
        let call = b.ins().call(func_ref, &call_args);
        let results = b.inst_results(call);

        let val = if results.is_empty() {
            b.ins().iconst(types::I64, 0)
        } else {
            results[0]
        };

        let tag = match ret_ty {
            Some(FllufType::Float) => Tag::Float,
            Some(FllufType::String) => Tag::Ptr,

            _ => Tag::Int,
        };

        Ok((TypedValue { val, tag }, compiled))
    }

    pub fn load_global(&mut self, b: &mut FunctionBuilder, mangled: &str) -> Value {
        let fod = self
            .module
            .get_name(mangled)
            .unwrap_or_else(|| panic!("global `{mangled}` not declared"));

        match fod {
            cranelift_module::FuncOrDataId::Data(id) => {
                let gv = self.module.declare_data_in_func(id, b.func);
                b.ins().symbol_value(types::I64, gv)
            }

            _ => panic!("`{mangled}` is not a data object"),
        }
    }
}
