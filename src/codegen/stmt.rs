use crate::codegen::*;
use crate::resolver::ModuleResolver;
use crate::syntax::ast::{FllufType, Stmt};
use anyhow::{Result, bail};
use cranelift::codegen::ir::types;
use cranelift::prelude::*;
use cranelift_frontend::FunctionBuilder;

impl Compiler {
    pub fn compile_stmt(
        &mut self,
        b: &mut FunctionBuilder,
        vars: &mut VarMap,
        s: &Stmt,
        resolver: &ModuleResolver,
        return_type: &FllufType,
    ) -> Result<()> {
        match s {
            Stmt::VarDecl {
                mut_,
                var_type,
                name,
                value,
            } => {
                let ir_ty = type_to_ir(var_type);
                let slot = b.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    ir_ty.bytes(),
                    0,
                ));

                let tv = self.compile_expr(b, vars, value, resolver)?;
                let converted = self.convert_type(b, tv, var_type);

                b.ins().stack_store(types::I64, converted, slot, 0);

                let tag = tag_from_type(var_type);

                vars.insert(name.clone(), (slot, tag, *mut_));
            }

            Stmt::Assign { name, value } => {
                let &(slot, tag, mut_) = vars
                    .get(name.as_str())
                    .ok_or_else(|| anyhow::anyhow!("undefined variable `{name}`"))?;

                if !mut_ {
                    bail!("cannot assign to immutable variable `{name}`");
                }

                let tv = self.compile_expr(b, vars, value, resolver)?;
                let ir_ty = type_to_ir_tag(tag);

                let converted = match tv.tag {
                    Tag::Float => b.ins().bitcast(types::I64, MemFlagsData::new(), tv.val),
                    _ => tv.val,
                };

                b.ins().stack_store(ir_ty, converted, slot, 0);
            }

            Stmt::Return(value) => {
                if let Some(tv) = self.detect_exit(value, b, vars, resolver)? {
                    self.emit_exit(b, tv.val);
                } else if *return_type == FllufType::Void {
                    self.compile_expr(b, vars, value, resolver)?;
                    b.ins().return_(&[]);
                } else {
                    let tv = self.compile_expr(b, vars, value, resolver)?;
                    b.ins().return_(&[tv.val]);
                }
            }

            Stmt::Expr(value) => {
                if let Some(tv) = self.detect_exit(value, b, vars, resolver)? {
                    self.emit_exit(b, tv.val);
                } else {
                    self.compile_expr(b, vars, value, resolver)?;
                }
            }

            Stmt::If {
                cond,
                then_block,
                else_ifs,
                else_block,
            } => {
                let end = b.create_block();
                let tv = self.compile_expr(b, vars, cond, resolver)?;
                let zero = b.ins().iconst(types::I64, 0);
                let is_true = b.ins().icmp(IntCC::NotEqual, tv.val, zero);

                let t_block = b.create_block();
                let chain = b.create_block();

                b.ins().brif(is_true, t_block, &[], chain, &[]);

                b.switch_to_block(t_block);
                b.seal_block(t_block);

                for s in &then_block.statements {
                    self.compile_stmt(b, vars, s, resolver, return_type)?;
                }

                if !self.block_has_terminator(b) {
                    b.ins().jump(end, &[]);
                }

                let mut cur = chain;

                for (ec, eb) in else_ifs {
                    b.switch_to_block(cur);
                    b.seal_block(cur);

                    let tvc = self.compile_expr(b, vars, ec, resolver)?;
                    let zero = b.ins().iconst(types::I64, 0);
                    let is_true = b.ins().icmp(IntCC::NotEqual, tvc.val, zero);

                    let tbn = b.create_block();
                    let next = b.create_block();

                    b.ins().brif(is_true, tbn, &[], next, &[]);

                    b.switch_to_block(tbn);
                    b.seal_block(tbn);

                    for s in &eb.statements {
                        self.compile_stmt(b, vars, s, resolver, return_type)?;
                    }

                    if !self.block_has_terminator(b) {
                        b.ins().jump(end, &[]);
                    }

                    cur = next;
                }

                b.switch_to_block(cur);
                b.seal_block(cur);

                if let Some(eb) = else_block {
                    for s in &eb.statements {
                        self.compile_stmt(b, vars, s, resolver, return_type)?;
                    }
                }

                if !self.block_has_terminator(b) {
                    b.ins().jump(end, &[]);
                }

                b.switch_to_block(end);
                b.seal_block(end);
            }
        }

        Ok(())
    }
}
