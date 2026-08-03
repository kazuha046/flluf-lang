use crate::codegen::*;
use crate::resolver::ModuleResolver;
use crate::syntax::ast::FllufType;
use anyhow::Result;
use cranelift::codegen::ir::types;
use cranelift::prelude::*;
use cranelift_frontend::FunctionBuilder;
use cranelift_module::{DataDescription, Linkage, Module};

impl Compiler {
    pub fn import(&mut self, name: &str, sig: &Signature) -> cranelift_module::FuncId {
        if self.imports.insert(name.to_string()) {
            self.module
                .declare_function(name, Linkage::Import, sig)
                .unwrap()
        } else {
            self.module
                .get_name(name)
                .and_then(|f| match f {
                    cranelift_module::FuncOrDataId::Func(id) => Some(id),
                    _ => None,
                })
                .expect("import not found")
        }
    }

    pub fn convert_type(
        &self,
        b: &mut FunctionBuilder,
        tv: TypedValue,
        target: &FllufType,
    ) -> Value {
        let ir_target = type_to_ir(target);

        if b.func.dfg.value_type(tv.val) == ir_target {
            return tv.val;
        }

        match (tv.tag, target) {
            (Tag::Int, FllufType::Float) => b.ins().fcvt_from_sint(types::F64, tv.val),
            (Tag::Float, FllufType::Int) => b.ins().fcvt_to_sint(types::I64, tv.val),

            _ => tv.val,
        }
    }

    pub fn check_types(
        &self,
        a: &TypedValue,
        b: &TypedValue,
        line: usize,
        resolver: &ModuleResolver,
    ) -> Result<()> {
        match (a.tag, b.tag) {
            (Tag::Ptr, _) | (_, Tag::Ptr) => {
                Err(self.err_at(resolver, line, "cannot use string in arithmetic"))
            }

            (a_tag, b_tag) if a_tag != b_tag => Err(self.err_at(
                resolver,
                line,
                format!("type mismatch: {a_tag:?} vs {b_tag:?}"),
            )),

            _ => Ok(()),
        }
    }

    pub fn check_assign(
        &self,
        target: &FllufType,
        value_tag: Tag,
        line: usize,
        resolver: &ModuleResolver,
        what: &str,
    ) -> Result<()> {
        let value_ty = match value_tag {
            Tag::Int => FllufType::Int,
            Tag::Float => FllufType::Float,
            Tag::Ptr => FllufType::String,
        };

        let ok = matches!(
            (&value_ty, target),
            (FllufType::Int, FllufType::Int)
                | (FllufType::Float, FllufType::Float)
                | (FllufType::String, FllufType::String)
                | (FllufType::Int, FllufType::Float)
        );

        if ok {
            Ok(())
        } else {
            Err(self.err_at(
                resolver,
                line,
                format!("type mismatch: cannot use {value_ty} as {target} {what}"),
            ))
        }
    }

    pub fn store_string(&mut self, b: &mut FunctionBuilder, s: &str) -> Value {
        let mut bytes = s.as_bytes().to_vec();

        bytes.push(0);

        let id = self.module.declare_anonymous_data(true, false).unwrap();
        let mut ctx = DataDescription::new();

        ctx.define(bytes.into());
        self.module.define_data(id, &ctx).unwrap();

        let gv = self.module.declare_data_in_func(id, b.func);

        b.ins().symbol_value(types::I64, gv)
    }
}
