#[cfg(target_os = "windows")]
use {
    crate::codegen::*,
    anyhow::Result,
    cranelift::{codegen::ir::types, prelude::*},
    cranelift_frontend::FunctionBuilder,
    cranelift_module::Module,
};

#[cfg(target_os = "windows")]
impl Compiler {
    pub fn compile_log_win(
        &mut self,
        b: &mut FunctionBuilder,
        tv: TypedValue,
    ) -> Result<TypedValue> {
        let slot =
            b.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 32, 0));

        let buf = b.ins().stack_addr(types::I64, slot, 0);
        let handle = self.emit_get_std_handle_win(b);

        let (start_ptr, total_len) = match tv.tag {
            Tag::Int => {
                let c30 = b.ins().iconst(types::I64, 30);
                let first_pos = self.emit_itoa_at_win(b, tv.val, buf, c30);

                self.emit_newline_win(b, buf, 31);

                let first_pos = self.emit_sign_win(b, tv.val, buf, first_pos);

                self.compute_range_win(b, buf, first_pos, 32)
            }

            Tag::Float => {
                let first_pos = self.emit_ftoa_win(b, tv.val, buf);

                self.emit_newline_win(b, buf, 31);

                let first_pos = self.emit_sign_win(b, tv.val, buf, first_pos);

                self.compute_range_win(b, buf, first_pos, 32)
            }

            Tag::Ptr => {
                let str_len = self.emit_strlen_win(b, tv.val);

                self.emit_write_file_win(b, handle, tv.val, str_len);

                let nl_buf = self.emit_newline_buf_win(b, buf);
                let one = b.ins().iconst(types::I64, 1);

                self.emit_write_file_win(b, handle, nl_buf, one);

                return Ok(TypedValue {
                    val: tv.val,
                    tag: tv.tag,
                });
            }
        };

        self.emit_write_file_win(b, handle, start_ptr, total_len);

        Ok(TypedValue {
            val: tv.val,
            tag: tv.tag,
        })
    }

    fn emit_newline_win(&mut self, b: &mut FunctionBuilder, buf: Value, off: i64) {
        let nl = b.ins().iconst(types::I8, b'\n' as i64);
        let off_val = b.ins().iconst(types::I64, off);
        let addr = b.ins().iadd(buf, off_val);

        b.ins().store(MemFlagsData::new(), nl, addr, 0);
    }

    fn emit_newline_buf_win(&mut self, b: &mut FunctionBuilder, buf: Value) -> Value {
        let nl = b.ins().iconst(types::I8, b'\n' as i64);

        b.ins().store(MemFlagsData::new(), nl, buf, 0);

        buf
    }

    fn compute_range_win(
        &mut self,
        b: &mut FunctionBuilder,
        buf: Value,
        first_pos: Value,
        end: i64,
    ) -> (Value, Value) {
        let start_ptr = b.ins().iadd(buf, first_pos);
        let end_val = b.ins().iconst(types::I64, end);
        let total_len = b.ins().isub(end_val, first_pos);

        (start_ptr, total_len)
    }

    fn emit_itoa_at_win(
        &mut self,
        b: &mut FunctionBuilder,
        val: Value,
        buf: Value,
        start_pos: Value,
    ) -> Value {
        let body = b.create_block();
        let done = b.create_block();

        b.ins().jump(body, &[start_pos.into(), val.into()]);
        b.switch_to_block(body);

        let pos = b.append_block_param(body, types::I64);
        let cur = b.append_block_param(body, types::I64);

        let ten = b.ins().iconst(types::I64, 10);
        let zero = b.ins().iconst(types::I64, 0);
        let one = b.ins().iconst(types::I64, 1);
        let ascii_zero = b.ins().iconst(types::I64, b'0' as i64);

        let digit_val = b.ins().srem(cur, ten);
        let digit_char = b.ins().iadd(digit_val, ascii_zero);
        let ascii_byte = b.ins().ireduce(types::I8, digit_char);

        let store_addr = b.ins().iadd(buf, pos);

        b.ins()
            .store(MemFlagsData::new(), ascii_byte, store_addr, 0);

        let new_cur = b.ins().sdiv(cur, ten);
        let next_pos = b.ins().isub(pos, one);
        let cmp = b.ins().icmp(IntCC::Equal, new_cur, zero);

        b.ins().brif(
            cmp,
            done,
            &[pos.into()],
            body,
            &[next_pos.into(), new_cur.into()],
        );

        b.switch_to_block(done);
        b.append_block_param(done, types::I64)
    }

    fn emit_sign_win(
        &mut self,
        b: &mut FunctionBuilder,
        val: Value,
        buf: Value,
        first_digit_pos: Value,
    ) -> Value {
        let is_neg = if b.func.dfg.value_type(val) == types::F64 {
            let zero_f64 = b.ins().f64const(Ieee64::with_float(0.0));

            b.ins().fcmp(FloatCC::LessThan, val, zero_f64)
        } else {
            let zero = b.ins().iconst(types::I64, 0);

            b.ins().icmp(IntCC::SignedLessThan, val, zero)
        };

        let one = b.ins().iconst(types::I64, 1);
        let sign_pos = b.ins().isub(first_digit_pos, one);
        let dash_addr = b.ins().iadd(buf, sign_pos);
        let dash = b.ins().iconst(types::I8, b'-' as i64);

        b.ins().store(MemFlagsData::new(), dash, dash_addr, 0);
        b.ins().select(is_neg, sign_pos, first_digit_pos)
    }

    fn emit_ftoa_win(&mut self, b: &mut FunctionBuilder, val: Value, buf: Value) -> Value {
        let abs_val = b.ins().fabs(val);
        let int_part_f = b.ins().trunc(abs_val);
        let int_part_i = b.ins().fcvt_to_sint(types::I64, int_part_f);

        let frac_f = b.ins().fsub(abs_val, int_part_f);
        let million = b.ins().f64const(Ieee64::with_float(1_000_000.0));
        let frac_scaled = b.ins().fmul(frac_f, million);
        let frac_i = b.ins().fcvt_to_sint(types::I64, frac_scaled);

        let dot_pos = b.ins().iconst(types::I64, 24);
        let int_start = b.ins().iconst(types::I64, 23);
        let int_first = self.emit_itoa_at_win(b, int_part_i, buf, int_start);

        let dot_addr = b.ins().iadd(buf, dot_pos);
        let dot_ch = b.ins().iconst(types::I8, b'.' as i64);

        b.ins().store(MemFlagsData::new(), dot_ch, dot_addr, 0);

        let six_start = b.ins().iconst(types::I64, 25);

        self.emit_6digit_win(b, frac_i, buf, six_start);

        int_first
    }

    fn emit_6digit_win(
        &mut self,
        b: &mut FunctionBuilder,
        val: Value,
        buf: Value,
        first_pos: Value,
    ) {
        let ten = b.ins().iconst(types::I64, 10);
        let ascii_zero = b.ins().iconst(types::I64, b'0' as i64);
        let mut cur = val;

        for i in (0..6).rev() {
            let i_val = b.ins().iconst(types::I64, i);
            let p = b.ins().iadd(first_pos, i_val);

            let digit_val = b.ins().srem(cur, ten);
            let digit_char = b.ins().iadd(digit_val, ascii_zero);
            let byte = b.ins().ireduce(types::I8, digit_char);

            let store_addr = b.ins().iadd(buf, p);

            b.ins().store(MemFlagsData::new(), byte, store_addr, 0);

            cur = b.ins().sdiv(cur, ten);
        }
    }

    fn emit_strlen_win(&mut self, b: &mut FunctionBuilder, ptr: Value) -> Value {
        let body = b.create_block();
        let done = b.create_block();
        let zero_init = b.ins().iconst(types::I64, 0);

        b.ins().jump(body, &[zero_init.into()]);
        b.switch_to_block(body);

        let idx = b.append_block_param(body, types::I64);
        let load_addr = b.ins().iadd(ptr, idx);
        let byte = b.ins().load(types::I8, MemFlagsData::new(), load_addr, 0);

        let zero_byte = b.ins().iconst(types::I8, 0);
        let is_zero = b.ins().icmp(IntCC::Equal, byte, zero_byte);

        let one = b.ins().iconst(types::I64, 1);
        let next_idx = b.ins().iadd(idx, one);

        b.ins()
            .brif(is_zero, done, &[idx.into()], body, &[next_idx.into()]);

        b.switch_to_block(done);
        b.append_block_param(done, types::I64)
    }

    fn emit_get_std_handle_win(&mut self, b: &mut FunctionBuilder) -> Value {
        let mut sig = self.module.make_signature();

        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));

        let id = self.import("GetStdHandle", &sig);
        let func_ref = self.module.declare_func_in_func(id, b.func);

        let n11 = b.ins().iconst(types::I64, -11);
        let call_inst = b.ins().call(func_ref, &[n11]);

        b.inst_results(call_inst)[0]
    }

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
}
