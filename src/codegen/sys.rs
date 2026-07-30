use crate::codegen::*;
use crate::syntax::ast::{Expr, FllufType};
use anyhow::{Result, bail};
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

    pub fn check_types(&self, a: &TypedValue, b: &TypedValue) -> Result<()> {
        match (a.tag, b.tag) {
            (Tag::Ptr, _) | (_, Tag::Ptr) => {
                bail!("cannot use string in arithmetic");
            }

            (a_tag, b_tag) if a_tag != b_tag => {
                bail!("type mismatch: {a_tag:?} vs {b_tag:?}");
            }

            _ => Ok(()),
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

    pub fn emit_exit(&mut self, b: &mut FunctionBuilder, arg: Value) {
        let mut sig = self.module.make_signature();

        sig.params.push(AbiParam::new(types::I64));

        #[cfg(not(target_os = "windows"))]
        let name = "exit";

        #[cfg(target_os = "windows")]
        let name = "ExitProcess";

        let id = self.import(name, &sig);
        let func_ref = self.module.declare_func_in_func(id, b.func);

        b.ins().call(func_ref, &[arg]);
    }

    pub fn detect_exit(
        &mut self,
        expr: &Expr,
        b: &mut FunctionBuilder,
        vars: &mut VarMap,
        use_system: bool,
    ) -> Result<Option<TypedValue>> {
        match expr {
            Expr::Call(name, args) if name == "Exit" => {
                let tv = self.compile_expr(b, vars, &args[0], use_system)?;

                Ok(Some(tv))
            }

            Expr::ModuleCall(mod_name, func_name, args)
                if mod_name == "System" && func_name == "Exit" =>
            {
                let tv = self.compile_expr(b, vars, &args[0], use_system)?;

                Ok(Some(tv))
            }

            _ => Ok(None),
        }
    }
}
