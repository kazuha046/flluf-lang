use crate::codegen::*;
use anyhow::{Result, bail};
use cranelift::codegen::ir::types;
use cranelift_frontend::FunctionBuilder;

impl Compiler {
    pub fn compile_expr(
        &mut self,
        b: &mut FunctionBuilder,
        vars: &mut VarMap,
        e: &Expr,
        use_system: bool,
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
                let &(slot, tag) = vars
                    .get(name.as_str())
                    .ok_or_else(|| anyhow::anyhow!("undefined variable `{name}`"))?;

                let ir_ty = type_to_ir_tag(tag);
                let val = b.ins().stack_load(types::I64, ir_ty, slot, 0);

                Ok(TypedValue { val, tag })
            }

            Expr::BinaryOp(l, op, r) => self.compile_binop(b, vars, l, *op, r, use_system),

            Expr::Call(name, args) if name == "Exit" => {
                bail!("'Exit' cannot be used as an expression");
            }

            Expr::Call(name, args) if name == "Log" => {
                if !use_system {
                    bail!("'Log' requires 'use System;'");
                }

                self.compile_log(b, vars, args, use_system)
            }

            Expr::Call(name, args) => self.compile_call(b, vars, name, args, use_system),

            Expr::ModuleCall(mod_name, func_name, args) => {
                if mod_name == "System" && func_name == "Exit" {
                    bail!("'Exit' cannot be used as an expression");
                } else if mod_name == "System" && func_name == "Log" {
                    self.compile_log(b, vars, args, use_system)
                } else {
                    bail!("unknown module function `{mod_name}::{func_name}`");
                }
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
        use_system: bool,
    ) -> Result<TypedValue> {
        let l = self.compile_expr(b, vars, left, use_system)?;
        let r = self.compile_expr(b, vars, right, use_system)?;

        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
                let is_float = l.tag == Tag::Float || r.tag == Tag::Float;

                if is_float {
                    self.check_types(&l, &r)?;

                    let lf = widen(b, l.val);
                    let rf = widen(b, r.val);

                    let val = match op {
                        BinOp::Add => b.ins().fadd(lf, rf),
                        BinOp::Sub => b.ins().fsub(lf, rf),
                        BinOp::Mul => b.ins().fmul(lf, rf),
                        BinOp::Div => b.ins().fdiv(lf, rf),

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

                    let lf = widen(b, l.val);
                    let rf = widen(b, r.val);

                    let cc = float_cc(op);
                    let cond = b.ins().fcmp(cc, lf, rf);

                    Ok(select_int(b, cond))
                } else {
                    self.check_types(&l, &r)?;

                    let cc = int_cc(op);
                    let cond = b.ins().icmp(cc, l.val, r.val);

                    Ok(select_int(b, cond))
                }
            }
        }
    }

    pub fn compile_log(
        &mut self,
        b: &mut FunctionBuilder,
        vars: &mut VarMap,
        args: &[Expr],
        use_system: bool,
    ) -> Result<TypedValue> {
        let tv = self.compile_expr(b, vars, &args[0], use_system)?;

        let (fn_name, ret_ty) = match tv.tag {
            Tag::Float => ("__flluf_log_f64", types::F64),
            Tag::Ptr => ("__flluf_log_ptr", types::I64),
            Tag::Int => ("__flluf_log_i64", types::I64),
        };

        let mut sig = self.module.make_signature();

        sig.params
            .push(AbiParam::new(b.func.dfg.value_type(tv.val)));
        sig.returns.push(AbiParam::new(ret_ty));

        let id = self.import(fn_name, &sig);
        let func_ref = self.module.declare_func_in_func(id, &mut b.func);
        let call = b.ins().call(func_ref, &[tv.val]);
        let val = b.inst_results(call)[0];

        Ok(TypedValue { val, tag: tv.tag })
    }

    pub fn compile_call(
        &mut self,
        b: &mut FunctionBuilder,
        vars: &mut VarMap,
        name: &str,
        args: &[Expr],
        use_system: bool,
    ) -> Result<TypedValue> {
        let callee = if name == "Main" { "main" } else { name };

        let (id, expected) = if let Some(fod) = self.module.get_name(callee) {
            match fod {
                cranelift_module::FuncOrDataId::Func(id) => {
                    let decl = self.module.declarations().get_function_decl(id);
                    let params: Vec<_> =
                        decl.signature.params.iter().map(|p| p.value_type).collect();

                    (id, params)
                }

                _ => bail!("`{callee}` is not a function"),
            }
        } else {
            let mut sig = self.module.make_signature();

            for a in args {
                let tv = self.compile_expr(b, vars, a, use_system)?;

                sig.params
                    .push(AbiParam::new(b.func.dfg.value_type(tv.val)));
            }

            sig.returns.push(AbiParam::new(types::I64));

            let id = self.import(callee, &sig);
            let params: Vec<_> = sig.params.iter().map(|p| p.value_type).collect();

            (id, params)
        };

        let mut compiled = Vec::new();

        for (i, a) in args.iter().enumerate() {
            let tv = self.compile_expr(b, vars, a, use_system)?;
            let val_ty = b.func.dfg.value_type(tv.val);
            let exp = expected.get(i).copied().unwrap_or(val_ty);

            let converted = match (val_ty, exp) {
                (types::I64, types::F64) => b.ins().fcvt_from_sint(types::F64, tv.val),
                (types::F64, types::I64) => b.ins().fcvt_to_sint(types::I64, tv.val),

                _ => tv.val,
            };

            compiled.push(converted);
        }

        let func_ref = self.module.declare_func_in_func(id, &mut b.func);
        let call = b.ins().call(func_ref, &compiled);
        let results = b.inst_results(call);

        let val = if results.is_empty() {
            b.ins().iconst(types::I64, 0)
        } else {
            results[0]
        };

        Ok(TypedValue { val, tag: Tag::Int })
    }
}
