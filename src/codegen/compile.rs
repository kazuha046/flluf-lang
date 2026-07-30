use crate::codegen::*;
use crate::syntax::ast::*;
use anyhow::{Result, bail};
use cranelift::codegen::ir::types;
use cranelift::prelude::*;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{FuncId, Linkage, Module};
use std::collections::HashMap;

impl Compiler {
    pub fn compile_program(&mut self, program: &Program) -> Result<()> {
        let ids: Vec<_> = program.functions.iter().map(|f| self.declare(f)).collect();

        let mut ctx = FunctionBuilderContext::new();

        for (f, id) in program.functions.iter().zip(ids) {
            self.compile_function(f, id, &mut ctx, program.use_system)?;
        }

        Ok(())
    }

    pub fn declare(&mut self, func: &Function) -> FuncId {
        let mut sig = self.module.make_signature();

        for p in &func.params {
            sig.params.push(AbiParam::new(type_to_ir(&p.param_type)));
        }

        if func.name == "Main" {
            sig.returns.push(AbiParam::new(types::I64));
        } else if let Some(t) = return_type_to_ir(&func.return_type) {
            sig.returns.push(AbiParam::new(t));
        }

        let entry = if func.name == "Main" {
            "main"
        } else {
            &func.name
        };

        self.module
            .declare_function(entry, Linkage::Export, &sig)
            .unwrap()
    }

    pub fn compile_function(
        &mut self,
        func: &Function,
        id: FuncId,
        ctx: &mut FunctionBuilderContext,
        use_system: bool,
    ) -> Result<()> {
        let mut data = self.module.make_context();

        let sig = self
            .module
            .declarations()
            .get_function_decl(id)
            .signature
            .clone();

        data.func.signature = sig;

        let mut builder = FunctionBuilder::new(&mut data.func, ctx);
        let entry = builder.create_block();

        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);

        let mut vars: VarMap = HashMap::new();
        let params = builder.block_params(entry).to_vec();

        for (i, p) in func.params.iter().enumerate() {
            let ir_ty = type_to_ir(&p.param_type);

            let slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                ir_ty.bytes(),
                0,
            ));

            builder.ins().stack_store(types::I64, params[i], slot, 0);

            let tag = tag_from_type(&p.param_type);

            vars.insert(p.name.clone(), (slot, tag));
        }

        for s in &func.body.statements {
            self.compile_stmt(&mut builder, &mut vars, s, use_system)?;
        }

        if !self.block_has_terminator(&builder) {
            match (&func.return_type, func.name.as_str()) {
                (_, "Main") => {
                    let z = builder.ins().iconst(types::I64, 0);

                    builder.ins().return_(&[z]);
                }

                (FllufType::Void, _) => {
                    builder.ins().return_(&[]);
                }

                _ => {
                    let ty = &func.return_type;

                    bail!(
                        "function `{}` must return a value of type `{ty}`",
                        func.name
                    );
                }
            }
        }

        let target_config = self.module.target_config();

        builder.finalize(target_config);

        self.module.define_function(id, &mut data)?;

        Ok(())
    }

    pub fn block_has_terminator(&self, b: &FunctionBuilder) -> bool {
        let block = match b.current_block() {
            Some(block) => block,
            None => return true,
        };

        let insts = b.func.layout.block_insts(block);
        let mut last = None;

        for inst in insts {
            last = Some(inst);
        }

        match last {
            None => false,
            Some(inst) => b.func.dfg.insts[inst].opcode().is_terminator(),
        }
    }
}
