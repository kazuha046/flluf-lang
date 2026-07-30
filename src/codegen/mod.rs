use crate::syntax::ast::*;
use anyhow::{Result, bail};
use cranelift::codegen::ir::{entities::StackSlot, types};
use cranelift::prelude::*;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{DataDescription, FuncId, Linkage, Module};
use std::collections::{HashMap, HashSet};

pub mod expr;
pub mod stmt;

pub type VarMap = HashMap<String, (StackSlot, Tag)>;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tag {
    Int,
    Float,
    Ptr,
}

pub struct TypedValue {
    pub val: Value,
    pub tag: Tag,
}

pub fn compile(program: &Program) -> Result<Vec<u8>> {
    let has_main = program.functions.iter().any(|f| f.name == "Main");

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
        imports: HashSet::new(),
    };

    comp.compile_program(program)?;

    let product = comp.module.finish();
    let data = product.emit().map_err(|e| anyhow::anyhow!("emit: {e}"))?;

    Ok(data)
}

pub struct Compiler {
    pub module: cranelift_object::ObjectModule,
    pub imports: HashSet<String>,
}

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

    pub fn import(&mut self, name: &str, sig: &Signature) -> FuncId {
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

pub fn tag_from_type(t: &FllufType) -> Tag {
    match t {
        FllufType::Float => Tag::Float,
        FllufType::String => Tag::Ptr,
        _ => Tag::Int,
    }
}

pub fn type_to_ir(t: &FllufType) -> Type {
    match t {
        FllufType::Int => types::I64,
        FllufType::Float => types::F64,
        FllufType::Void => types::I64,
        FllufType::String => types::I64,
    }
}

pub fn type_to_ir_tag(t: Tag) -> Type {
    match t {
        Tag::Int => types::I64,
        Tag::Float => types::F64,
        Tag::Ptr => types::I64,
    }
}

pub fn return_type_to_ir(t: &FllufType) -> Option<Type> {
    match t {
        FllufType::Int => Some(types::I64),
        FllufType::Float => Some(types::F64),
        FllufType::Void => None,
        FllufType::String => Some(types::I64),
    }
}

pub fn widen(b: &mut FunctionBuilder, v: Value) -> Value {
    if b.func.dfg.value_type(v) == types::I64 {
        b.ins().fcvt_from_sint(types::F64, v)
    } else {
        v
    }
}

pub fn int_cc(op: BinOp) -> IntCC {
    use BinOp::*;

    match op {
        Eq => IntCC::Equal,
        Ne => IntCC::NotEqual,
        Lt => IntCC::SignedLessThan,
        Gt => IntCC::SignedGreaterThan,
        Le => IntCC::SignedLessThanOrEqual,
        Ge => IntCC::SignedGreaterThanOrEqual,

        _ => unreachable!(),
    }
}

pub fn float_cc(op: BinOp) -> FloatCC {
    use BinOp::*;

    match op {
        Eq => FloatCC::Equal,
        Ne => FloatCC::NotEqual,
        Lt => FloatCC::LessThan,
        Gt => FloatCC::GreaterThan,
        Le => FloatCC::LessThanOrEqual,
        Ge => FloatCC::GreaterThanOrEqual,

        _ => unreachable!(),
    }
}

pub fn select_int(b: &mut FunctionBuilder, cond: Value) -> TypedValue {
    let one = b.ins().iconst(types::I64, 1);
    let zero = b.ins().iconst(types::I64, 0);

    TypedValue {
        val: b.ins().select(cond, one, zero),
        tag: Tag::Int,
    }
}
