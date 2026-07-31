use crate::codegen::*;
use crate::resolver::ModuleResolver;
use crate::syntax::ast::*;
use anyhow::{Result, bail};
use cranelift::codegen::ir::types;
use cranelift::prelude::*;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{DataDescription, FuncId, Linkage, Module};
use std::collections::HashMap;

impl Compiler {
    pub fn compile_program(&mut self, resolver: &ModuleResolver) -> Result<()> {
        let func_ids: Vec<_> = resolver
            .all_functions
            .iter()
            .map(|(mangled, f)| self.declare(mangled, f))
            .collect();

        let global_ids: Vec<_> = resolver
            .all_globals
            .iter()
            .map(|(mangled, g)| self.declare_global(mangled, g))
            .collect();

        let mut ctx = FunctionBuilderContext::new();

        for ((mangled, f), id) in resolver.all_functions.iter().zip(&func_ids) {
            self.compile_function(mangled, f, *id, &mut ctx, resolver)?;
        }

        for ((_, g), id) in resolver.all_globals.iter().zip(&global_ids) {
            self.define_global(g, *id);
        }

        Ok(())
    }

    pub fn declare(&mut self, mangled: &str, func: &Function) -> FuncId {
        let mut sig = self.module.make_signature();

        for p in &func.params {
            sig.params.push(AbiParam::new(type_to_ir(&p.param_type)));
        }

        if mangled == "main" {
            sig.returns.push(AbiParam::new(types::I64));
        } else if let Some(t) = return_type_to_ir(&func.return_type) {
            sig.returns.push(AbiParam::new(t));
        }

        self.module
            .declare_function(mangled, Linkage::Export, &sig)
            .unwrap()
    }

    pub fn declare_global(&mut self, mangled: &str, _g: &GlobalVar) -> cranelift_module::DataId {
        let writable = false;
        self.module
            .declare_data(mangled, Linkage::Export, writable, false)
            .unwrap()
    }

    pub fn define_global(&mut self, g: &GlobalVar, id: cranelift_module::DataId) {
        let bytes = match &g.value {
            Expr::StringLit(s) => {
                let mut b = s.as_bytes().to_vec();
                b.push(0);
                b.into()
            }

            _ => match &g.var_type {
                FllufType::Int => {
                    if let Expr::IntLit(n) = g.value {
                        let mut b = Vec::with_capacity(8);

                        b.extend_from_slice(&n.to_le_bytes());

                        b.into()
                    } else {
                        Vec::new().into()
                    }
                }
                FllufType::Float => {
                    if let Expr::FloatLit(n) = g.value {
                        let mut b = Vec::with_capacity(8);

                        b.extend_from_slice(&n.to_bits().to_le_bytes());

                        b.into()
                    } else {
                        Vec::new().into()
                    }
                }

                _ => Vec::new().into(),
            },
        };

        let mut ctx = DataDescription::new();

        ctx.define(bytes);
        self.module.define_data(id, &ctx).unwrap();
    }

    pub fn compile_function(
        &mut self,
        mangled: &str,
        func: &Function,
        id: FuncId,
        ctx: &mut FunctionBuilderContext,
        resolver: &ModuleResolver,
    ) -> Result<()> {
        self.use_system = resolver
            .function_system
            .get(mangled)
            .copied()
            .unwrap_or(false);

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

            vars.insert(p.name.clone(), (slot, tag, p.mut_));
        }

        for s in &func.body.statements {
            self.compile_stmt(&mut builder, &mut vars, s, resolver, &func.return_type)?;
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
                    bail!(
                        "function `{}` must return a value of type `{}`",
                        func.name,
                        func.return_type
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
