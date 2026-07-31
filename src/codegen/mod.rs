use crate::resolver::ModuleResolver;
use anyhow::{Result, bail};
use cranelift::codegen::ir::entities::StackSlot;
use cranelift::codegen::settings;

pub mod compile;
pub mod expr;
pub mod log_win;
pub mod stmt;
pub mod sys;
pub mod types;

pub use types::{
    float_cc, int_cc, return_type_to_ir, select_int, tag_from_type, type_to_ir, type_to_ir_tag,
    widen,
};

pub type VarMap = std::collections::HashMap<String, (StackSlot, Tag, bool)>;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tag {
    Int,
    Float,
    Ptr,
}

pub struct TypedValue {
    pub val: cranelift::prelude::Value,
    pub tag: Tag,
}

pub struct Compiler {
    pub module: cranelift_object::ObjectModule,
    pub imports: std::collections::HashSet<String>,
}

pub fn compile(resolver: &ModuleResolver) -> Result<Vec<u8>> {
    let has_main = resolver.all_functions.iter().any(|(n, _)| n == "main");

    if !has_main {
        bail!("program must have a 'Main' function");
    }

    let isa_builder = cranelift_native::builder()
        .map_err(|e| anyhow::anyhow!("failed to create native isa builder: {e}"))?;

    let flags = settings::Flags::new(settings::builder());

    let isa = isa_builder
        .finish(flags)
        .map_err(|e| anyhow::anyhow!("isa finish: {e}"))?;

    let obj_builder = cranelift_object::ObjectBuilder::new(
        isa,
        "flluf",
        cranelift_module::default_libcall_names(),
    )
    .map_err(|e| anyhow::anyhow!("object builder: {e}"))?;

    let module = cranelift_object::ObjectModule::new(obj_builder);

    let mut comp = Compiler {
        module,
        imports: std::collections::HashSet::new(),
    };

    comp.compile_program(resolver)?;

    let product = comp.module.finish();
    let data = product.emit().map_err(|e| anyhow::anyhow!("emit: {e}"))?;

    Ok(data)
}
