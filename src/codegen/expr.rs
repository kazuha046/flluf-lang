use crate::codegen::*;
use anyhow::{Result, bail};
use cranelift::codegen::ir::types;
use cranelift_frontend::FunctionBuilder;

#[cfg(target_os = "windows")]
use cranelift::codegen::ir::BlockArg;

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

            Expr::Call(name, _) if name == "Exit" => {
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

    #[cfg(target_os = "windows")]
    fn compile_log_win(&mut self, b: &mut FunctionBuilder, tv: TypedValue) -> Result<TypedValue> {
        let slot =
            b.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 32, 0));

        let buf = b.ins().stack_addr(types::I64, slot, 0);
        let handle = self.emit_get_std_handle_win(b);

        match tv.tag {
            Tag::Int => {
                let c30 = b.ins().iconst(types::I64, 30);
                let first_pos = self.emit_itoa_at_win(b, tv.val, buf, c30);

                let c31 = b.ins().iconst(types::I64, 31);
                let nl_addr = b.ins().iadd(buf, c31);
                let nl = b.ins().iconst(types::I8, b'\n' as i64);

                b.ins().store(MemFlagsData::new(), nl, nl_addr, 0);

                let first_pos = self.emit_sign_win(b, tv.val, buf, first_pos);
                let c32 = b.ins().iconst(types::I64, 32);
                let total_len = b.ins().isub(c32, first_pos);
                let start_ptr = b.ins().iadd(buf, first_pos);

                self.emit_write_file_win(b, handle, start_ptr, total_len);
            }

            Tag::Float => {
                let first_pos = self.emit_ftoa_win(b, tv.val, buf);

                let c31 = b.ins().iconst(types::I64, 31);
                let nl_addr = b.ins().iadd(buf, c31);
                let nl = b.ins().iconst(types::I8, b'\n' as i64);

                b.ins().store(MemFlagsData::new(), nl, nl_addr, 0);

                let first_pos = self.emit_sign_win(b, tv.val, buf, first_pos);
                let c32 = b.ins().iconst(types::I64, 32);
                let total_len = b.ins().isub(c32, first_pos);
                let start_ptr = b.ins().iadd(buf, first_pos);

                self.emit_write_file_win(b, handle, start_ptr, total_len);
            }

            Tag::Ptr => {
                let str_len = self.emit_strlen_win(b, tv.val);
                self.emit_write_file_win(b, handle, tv.val, str_len);

                let nl = b.ins().iconst(types::I8, b'\n' as i64);
                b.ins().store(MemFlagsData::new(), nl, buf, 0);
                let one = b.ins().iconst(types::I64, 1);

                self.emit_write_file_win(b, handle, buf, one);
            }
        }

        Ok(TypedValue {
            val: tv.val,
            tag: tv.tag,
        })
    }

    #[cfg(target_os = "windows")]
    fn emit_itoa_at_win(
        &mut self,
        b: &mut FunctionBuilder,
        val: Value,
        buf: Value,
        start_pos: Value,
    ) -> Value {
        let body = b.create_block();
        let done = b.create_block();

        let args: [BlockArg; 2] = [start_pos.into(), val.into()];

        b.ins().jump(body, &args);

        b.switch_to_block(body);

        let pos = b.append_block_param(body, types::I64);
        let cur = b.append_block_param(body, types::I64);

        let ten = b.ins().iconst(types::I64, 10);
        let zero = b.ins().iconst(types::I64, 0);
        let one = b.ins().iconst(types::I64, 1);
        let cur0 = b.ins().iconst(types::I64, b'0' as i64);

        let digit = b.ins().srem(cur, ten);
        let ascii_digit = b.ins().iadd(digit, cur0);
        let ascii_byte = b.ins().ireduce(types::I8, ascii_digit);
        let store_addr = b.ins().iadd(buf, pos);

        b.ins()
            .store(MemFlagsData::new(), ascii_byte, store_addr, 0);

        let new_cur = b.ins().sdiv(cur, ten);
        let is_zero = b.ins().icmp(IntCC::Equal, new_cur, zero);

        let next_pos = b.ins().isub(pos, one);

        let done_args: [BlockArg; 1] = [pos.into()];
        let body_args: [BlockArg; 2] = [next_pos.into(), new_cur.into()];

        b.ins().brif(is_zero, done, &done_args, body, &body_args);

        b.switch_to_block(done);
        b.append_block_param(done, types::I64)
    }

    #[cfg(target_os = "windows")]
    fn emit_sign_win(
        &mut self,
        b: &mut FunctionBuilder,
        val: Value,
        buf: Value,
        first_digit_pos: Value,
    ) -> Value {
        let is_neg = if b.func.dfg.value_type(val) == types::F64 {
            let f0 = b.ins().f64const(Ieee64::with_float(0.0));
            b.ins().fcmp(FloatCC::LessThan, val, f0)
        } else {
            let i0 = b.ins().iconst(types::I64, 0);
            b.ins().icmp(IntCC::SignedLessThan, val, i0)
        };

        let one = b.ins().iconst(types::I64, 1);
        let dash = b.ins().iconst(types::I8, b'-' as i64);
        let sign_pos = b.ins().isub(first_digit_pos, one);
        let dash_addr = b.ins().iadd(buf, sign_pos);

        b.ins().store(MemFlagsData::new(), dash, dash_addr, 0);
        b.ins().select(is_neg, sign_pos, first_digit_pos)
    }

    #[cfg(target_os = "windows")]
    fn emit_ftoa_win(&mut self, b: &mut FunctionBuilder, val: Value, buf: Value) -> Value {
        let abs_val = b.ins().fabs(val);
        let int_part_f = b.ins().trunc(abs_val);
        let int_part_i = b.ins().fcvt_to_sint(types::I64, int_part_f);

        let frac_f = b.ins().fsub(abs_val, int_part_f);
        let million_f = b.ins().f64const(Ieee64::with_float(1_000_000.0));
        let frac_scaled = b.ins().fmul(frac_f, million_f);
        let frac_i = b.ins().fcvt_to_sint(types::I64, frac_scaled);

        let c23 = b.ins().iconst(types::I64, 23);
        let int_first = self.emit_itoa_at_win(b, int_part_i, buf, c23);

        let c24 = b.ins().iconst(types::I64, 24);
        let dot_addr = b.ins().iadd(buf, c24);
        let dot = b.ins().iconst(types::I8, b'.' as i64);

        b.ins().store(MemFlagsData::new(), dot, dot_addr, 0);

        let c25 = b.ins().iconst(types::I64, 25);

        self.emit_6digit_win(b, frac_i, buf, c25);

        int_first
    }

    #[cfg(target_os = "windows")]
    fn emit_6digit_win(
        &mut self,
        b: &mut FunctionBuilder,
        val: Value,
        buf: Value,
        first_pos: Value,
    ) {
        let ten = b.ins().iconst(types::I64, 10);
        let cur0 = b.ins().iconst(types::I64, b'0' as i64);

        let mut cur = val;

        for i in (0..6).rev() {
            let off = b.ins().iconst(types::I64, i);
            let p = b.ins().iadd(first_pos, off);
            let d = b.ins().srem(cur, ten);
            let c = b.ins().iadd(d, cur0);
            let b8 = b.ins().ireduce(types::I8, c);
            let store_addr = b.ins().iadd(buf, p);

            b.ins().store(MemFlagsData::new(), b8, store_addr, 0);

            cur = b.ins().sdiv(cur, ten);
        }
    }

    #[cfg(target_os = "windows")]
    fn emit_strlen_win(&mut self, b: &mut FunctionBuilder, ptr: Value) -> Value {
        let body = b.create_block();
        let done = b.create_block();

        let c0 = b.ins().iconst(types::I64, 0);
        let args: [BlockArg; 1] = [c0.into()];

        b.ins().jump(body, &args);

        b.switch_to_block(body);

        let idx = b.append_block_param(body, types::I64);
        let addr = b.ins().iadd(ptr, idx);
        let byte = b.ins().load(types::I8, MemFlagsData::new(), addr, 0);
        let i80 = b.ins().iconst(types::I8, 0);
        let is_zero = b.ins().icmp(IntCC::Equal, byte, i80);

        let done_args: [BlockArg; 1] = [idx.into()];
        let one = b.ins().iconst(types::I64, 1);
        let next = b.ins().iadd(idx, one);
        let body_args: [BlockArg; 1] = [next.into()];

        b.ins().brif(is_zero, done, &done_args, body, &body_args);

        b.switch_to_block(done);
        b.append_block_param(done, types::I64)
    }

    #[cfg(target_os = "windows")]
    fn emit_get_std_handle_win(&mut self, b: &mut FunctionBuilder) -> Value {
        let mut sig = self.module.make_signature();

        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));

        let id = self.import("GetStdHandle", &sig);
        let func_ref = self.module.declare_func_in_func(id, b.func);

        let neg11 = b.ins().iconst(types::I64, -11);
        let call = b.ins().call(func_ref, &[neg11]);

        b.inst_results(call)[0]
    }

    #[cfg(target_os = "windows")]
    fn emit_write_file_win(
        &mut self,
        b: &mut FunctionBuilder,
        handle: Value,
        buf: Value,
        len: Value,
    ) {
        let bytes_written_slot =
            b.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 0));

        let bytes_written_ptr = b.ins().stack_addr(types::I64, bytes_written_slot, 0);

        let mut sig = self.module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));

        let id = self.import("WriteFile", &sig);
        let func_ref = self.module.declare_func_in_func(id, b.func);

        let zero = b.ins().iconst(types::I64, 0);

        b.ins()
            .call(func_ref, &[handle, buf, len, bytes_written_ptr, zero]);
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
}
