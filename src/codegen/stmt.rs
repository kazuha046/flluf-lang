use crate::codegen::*;
use crate::resolver::ModuleResolver;
use crate::syntax::ast::{Expr, FllufType, Stmt};
use anyhow::Result;
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
        is_main: bool,
    ) -> Result<()> {
        match s {
            Stmt::VarDecl {
                mut_,
                var_type,
                name,
                value,
                line,
            } => {
                let ir_ty = type_to_ir(var_type);

                let slot = b.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    ir_ty.bytes(),
                    0,
                ));

                let tv = self.compile_expr(b, vars, value, resolver)?;

                self.check_assign(
                    var_type,
                    tv.tag,
                    *line,
                    resolver,
                    &format!("for variable `{name}`"),
                )?;

                let converted = self.convert_type(b, tv, var_type);

                b.ins().stack_store(types::I64, converted, slot, 0);

                let tag = tag_from_type(var_type);

                vars.insert(name.clone(), (slot, tag, *mut_, var_type.clone()));
            }

            Stmt::Assign { name, value, line } => {
                let &(slot, tag, mut_, ref declared_ty) =
                    vars.get(name.as_str()).ok_or_else(|| {
                        self.err_at(resolver, *line, format!("undefined variable `{name}`"))
                    })?;

                let declared_ty = declared_ty.clone();

                if !mut_ {
                    return Err(self.err_at(
                        resolver,
                        *line,
                        format!("cannot assign to immutable variable `{name}`"),
                    ));
                }

                let tv = self.compile_expr(b, vars, value, resolver)?;

                self.check_assign(
                    &declared_ty,
                    tv.tag,
                    *line,
                    resolver,
                    &format!("to variable `{name}`"),
                )?;

                let ir_ty = type_to_ir_tag(tag);

                let converted = match tv.tag {
                    Tag::Float => b.ins().bitcast(types::I64, MemFlagsData::new(), tv.val),
                    _ => tv.val,
                };

                b.ins().stack_store(ir_ty, converted, slot, 0);
            }

            Stmt::Return(value, line) => {
                if *return_type == FllufType::Void {
                    self.compile_expr(b, vars, value, resolver)?;

                    if is_main {
                        let z = b.ins().iconst(types::I64, 0);
                        b.ins().return_(&[z]);
                    } else {
                        b.ins().return_(&[]);
                    }
                } else {
                    let tv = self.compile_expr(b, vars, value, resolver)?;

                    self.check_assign(return_type, tv.tag, *line, resolver, "as return value")?;

                    b.ins().return_(&[tv.val]);
                }
            }

            Stmt::Expr(value, _line) => {
                self.compile_expr(b, vars, value, resolver)?;
            }

            Stmt::If {
                cond,
                then_block,
                else_ifs,
                else_block,
            } => {
                let end = b.create_block();

                let (t_block, chain) = self.cond_brif(b, vars, cond, resolver)?;

                self.finish_block(
                    b,
                    vars,
                    t_block,
                    end,
                    &then_block.statements,
                    resolver,
                    return_type,
                    is_main,
                )?;

                let mut cur = chain;

                for (ec, eb) in else_ifs {
                    b.switch_to_block(cur);
                    b.seal_block(cur);

                    let (tbn, next) = self.cond_brif(b, vars, ec, resolver)?;

                    self.finish_block(
                        b,
                        vars,
                        tbn,
                        end,
                        &eb.statements,
                        resolver,
                        return_type,
                        is_main,
                    )?;

                    cur = next;
                }

                b.switch_to_block(cur);
                b.seal_block(cur);

                if let Some(eb) = else_block {
                    for s in &eb.statements {
                        self.compile_stmt(b, vars, s, resolver, return_type, is_main)?;
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

    fn cond_brif(
        &mut self,
        b: &mut FunctionBuilder,
        vars: &mut VarMap,
        cond: &Expr,
        resolver: &ModuleResolver,
    ) -> Result<(Block, Block)> {
        let tv = self.compile_expr(b, vars, cond, resolver)?;
        let zero = b.ins().iconst(types::I64, 0);
        let is_true = b.ins().icmp(IntCC::NotEqual, tv.val, zero);

        let t_block = b.create_block();
        let f_block = b.create_block();

        b.ins().brif(is_true, t_block, &[], f_block, &[]);

        Ok((t_block, f_block))
    }

    #[allow(clippy::too_many_arguments)]
    fn finish_block(
        &mut self,
        b: &mut FunctionBuilder,
        vars: &mut VarMap,
        block: Block,
        end: Block,
        statements: &[Stmt],
        resolver: &ModuleResolver,
        return_type: &FllufType,
        is_main: bool,
    ) -> Result<()> {
        b.switch_to_block(block);
        b.seal_block(block);

        for s in statements {
            self.compile_stmt(b, vars, s, resolver, return_type, is_main)?;
        }

        if !self.block_has_terminator(b) {
            b.ins().jump(end, &[]);
        }

        Ok(())
    }
}
