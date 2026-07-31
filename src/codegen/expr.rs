use crate::codegen::*;
use crate::resolver::ModuleResolver;
use crate::syntax::ast::{BinOp, Expr};
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

            Expr::Call(name, _) if name == "Exit" => {
                bail!("'Exit' cannot be used as an expression");
            }

            Expr::Call(name, args) if name == "Log" => {
                if !self.use_system {
                    bail!("'Log' requires 'use System;'");
                }

                self.compile_log(b, vars, args, resolver)
            }

            Expr::Call(name, args) => self.compile_call(b, vars, name, args, resolver),

            Expr::ModuleCall(mod_name, func_name, args) => {
                if mod_name == "System" && func_name == "Exit" {
                    bail!("'Exit' cannot be used as an expression");
                } else if mod_name == "System" && func_name == "Log" {
                    self.compile_log(b, vars, args, resolver)
                } else {
                    let key = format!("{}::{}", mod_name, func_name);

                    let mangled = resolver
                        .func_map
                        .get(&key)
                        .ok_or_else(|| anyhow::anyhow!("unknown function `{key}`"))?;

                    self.compile_call(b, vars, mangled, args, resolver)
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

    pub fn compile_log(
        &mut self,
        b: &mut FunctionBuilder,
        vars: &mut VarMap,
        args: &[Expr],
        resolver: &ModuleResolver,
    ) -> Result<TypedValue> {
        let tv = self.compile_expr(b, vars, &args[0], resolver)?;

        #[cfg(not(target_os = "windows"))]
        return self.compile_log_inner(b, tv);

        #[cfg(target_os = "windows")]
        self.compile_log_win(b, tv)
    }

    #[cfg(not(target_os = "windows"))]
    fn compile_log_inner(&mut self, b: &mut FunctionBuilder, tv: TypedValue) -> Result<TypedValue> {
        let fmt_str = match tv.tag {
            Tag::Int => "%lld\n\0",
            Tag::Float => "%f\n\0",
            Tag::Ptr => "%s\n\0",
        };

        let fmt_ptr = self.store_string(b, fmt_str);

        let val = match tv.tag {
            Tag::Float => b.ins().bitcast(types::I64, MemFlagsData::new(), tv.val),
            _ => tv.val,
        };

        let mut sig = self.module.make_signature();

        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));

        let printf_id = self.import("printf", &sig);
        let printf_ref = self.module.declare_func_in_func(printf_id, b.func);

        b.ins().call(printf_ref, &[fmt_ptr, val]);

        Ok(TypedValue {
            val: tv.val,
            tag: tv.tag,
        })
    }

    pub fn compile_call(
        &mut self,
        b: &mut FunctionBuilder,
        vars: &mut VarMap,
        name: &str,
        args: &[Expr],
        resolver: &ModuleResolver,
    ) -> Result<TypedValue> {
        let (id, expected) = if let Some(fod) = self.module.get_name(name) {
            match fod {
                cranelift_module::FuncOrDataId::Func(id) => {
                    let decl = self.module.declarations().get_function_decl(id);

                    let params: Vec<_> =
                        decl.signature.params.iter().map(|p| p.value_type).collect();

                    (id, params)
                }

                _ => bail!("`{name}` is not a function"),
            }
        } else if let Some(resolved) = resolver.func_map.get(name) {
            match self.module.get_name(resolved) {
                Some(cranelift_module::FuncOrDataId::Func(id)) => {
                    let decl = self.module.declarations().get_function_decl(id);

                    let params: Vec<_> =
                        decl.signature.params.iter().map(|p| p.value_type).collect();

                    (id, params)
                }

                _ => bail!("`{name}` resolves to an unknown function"),
            }
        } else {
            let mut sig = self.module.make_signature();

            for a in args {
                let tv = self.compile_expr(b, vars, a, resolver)?;

                sig.params
                    .push(AbiParam::new(b.func.dfg.value_type(tv.val)));
            }

            sig.returns.push(AbiParam::new(types::I64));

            let id = self.import(name, &sig);
            let params: Vec<_> = sig.params.iter().map(|p| p.value_type).collect();

            (id, params)
        };

        let mut compiled = Vec::new();

        for (i, a) in args.iter().enumerate() {
            let tv = self.compile_expr(b, vars, a, resolver)?;
            let val_ty = b.func.dfg.value_type(tv.val);
            let exp = expected.get(i).copied().unwrap_or(val_ty);

            compiled.push(match (val_ty, exp) {
                (types::I64, types::F64) => b.ins().fcvt_from_sint(types::F64, tv.val),
                (types::F64, types::I64) => b.ins().fcvt_to_sint(types::I64, tv.val),
                _ => tv.val,
            });
        }

        let func_ref = self.module.declare_func_in_func(id, b.func);
        let call = b.ins().call(func_ref, &compiled);
        let results = b.inst_results(call);

        let val = if results.is_empty() {
            b.ins().iconst(types::I64, 0)
        } else {
            results[0]
        };

        Ok(TypedValue { val, tag: Tag::Int })
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
