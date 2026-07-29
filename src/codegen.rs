use crate::syntax::ast::*;
use anyhow::{Result, bail};
use cranelift::codegen::ir::{entities::StackSlot, types};
use cranelift::prelude::*;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{DataDescription, FuncId, Linkage, Module};
use std::collections::{HashMap, HashSet};

type VarMap = HashMap<String, (StackSlot, Tag)>;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Tag {
    Int,
    Float,
    Ptr,
}

struct TypedValue {
    val: Value,
    tag: Tag,
}

pub fn compile(program: &Program) -> Result<Vec<u8>> {
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

struct Compiler {
    module: cranelift_object::ObjectModule,
    imports: HashSet<String>,
}

impl Compiler {
    fn compile_program(&mut self, program: &Program) -> Result<()> {
        let ids: Vec<_> = program.functions.iter().map(|f| self.declare(f)).collect();

        let mut ctx = FunctionBuilderContext::new();

        for (f, id) in program.functions.iter().zip(ids) {
            self.compile_function(f, id, &mut ctx, program.use_system)?;
        }

        Ok(())
    }

    fn declare(&mut self, func: &Function) -> FuncId {
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

    fn compile_function(
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

            let tag = match &p.param_type {
                FllufType::Float => Tag::Float,
                _ => Tag::Int,
            };

            vars.insert(p.name.clone(), (slot, tag));
        }

        for s in &func.body.statements {
            self.compile_stmt(&mut builder, &mut vars, s, use_system)?;
        }

        match (&func.return_type, func.name.as_str()) {
            (_, "Main") => {
                let z = builder.ins().iconst(types::I64, 0);

                builder.ins().return_(&[z]);
            }

            (FllufType::Void, _) => {
                builder.ins().return_(&[]);
            }

            _ => {}
        }

        let target_config = self.module.target_config();
        builder.finalize(target_config);

        self.module.define_function(id, &mut data)?;

        Ok(())
    }

    fn compile_stmt(
        &mut self,
        b: &mut FunctionBuilder,
        vars: &mut VarMap,
        s: &Stmt,
        use_system: bool,
    ) -> Result<()> {
        match s {
            Stmt::VarDecl {
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

                let tv = self.compile_expr(b, vars, value, use_system)?;
                let converted = self.convert_type(b, tv, var_type);

                b.ins().stack_store(types::I64, converted, slot, 0);

                let tag = match var_type {
                    FllufType::Float => Tag::Float,
                    _ => Tag::Int,
                };

                vars.insert(name.clone(), (slot, tag));
            }

            Stmt::Return(value) => {
                if let Some(tv) = self.detect_exit(value, b, vars, use_system)? {
                    self.emit_exit(b, tv.val);
                } else {
                    let tv = self.compile_expr(b, vars, value, use_system)?;

                    b.ins().return_(&[tv.val]);
                }
            }

            Stmt::Expr(value) => {
                if let Some(tv) = self.detect_exit(value, b, vars, use_system)? {
                    self.emit_exit(b, tv.val);
                } else {
                    self.compile_expr(b, vars, value, use_system)?;
                }
            }

            Stmt::If {
                cond,
                then_block,
                else_ifs,
                else_block,
            } => {
                let end = b.create_block();
                let tv = self.compile_expr(b, vars, cond, use_system)?;
                let zero = b.ins().iconst(types::I64, 0);
                let is_true = b.ins().icmp(IntCC::NotEqual, tv.val, zero);

                let t_block = b.create_block();
                let chain = b.create_block();

                b.ins().brif(is_true, t_block, &[], chain, &[]);

                b.switch_to_block(t_block);
                b.seal_block(t_block);

                for s in &then_block.statements {
                    self.compile_stmt(b, vars, s, use_system)?;
                }

                b.ins().jump(end, &[]);

                let mut cur = chain;

                for (ec, eb) in else_ifs {
                    b.switch_to_block(cur);
                    b.seal_block(cur);

                    let tvc = self.compile_expr(b, vars, ec, use_system)?;
                    let zero = b.ins().iconst(types::I64, 0);
                    let is_true = b.ins().icmp(IntCC::NotEqual, tvc.val, zero);

                    let tbn = b.create_block();
                    let next = b.create_block();

                    b.ins().brif(is_true, tbn, &[], next, &[]);

                    b.switch_to_block(tbn);
                    b.seal_block(tbn);

                    for s in &eb.statements {
                        self.compile_stmt(b, vars, s, use_system)?;
                    }

                    b.ins().jump(end, &[]);

                    cur = next;
                }

                b.switch_to_block(cur);
                b.seal_block(cur);

                if let Some(eb) = else_block {
                    for s in &eb.statements {
                        self.compile_stmt(b, vars, s, use_system)?;
                    }
                }

                b.ins().jump(end, &[]);

                b.switch_to_block(end);
                b.seal_block(end);
            }
        }

        Ok(())
    }

    fn compile_expr(
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

                let ir_ty = match tag {
                    Tag::Int => types::I64,
                    Tag::Float => types::F64,
                    Tag::Ptr => types::I64,
                };

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

    fn compile_binop(
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

    fn check_types(&self, a: &TypedValue, b: &TypedValue) -> Result<()> {
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

    fn compile_log(
        &mut self,
        b: &mut FunctionBuilder,
        vars: &mut VarMap,
        args: &[Expr],
        use_system: bool,
    ) -> Result<TypedValue> {
        let tv = self.compile_expr(b, vars, &args[0], use_system)?;

        let (fn_name, ret_ty) = match tv.tag {
            Tag::Float => ("__flluf_log_f64", types::F64),
            Tag::Ptr => ("__flluf_log_ptr", types::I64),
            Tag::Int => ("__flluf_log_i64", types::I64),
        };

        let mut sig = self.module.make_signature();

        sig.params
            .push(AbiParam::new(b.func.dfg.value_type(tv.val)));
        sig.returns.push(AbiParam::new(ret_ty));

        let id = self.import(fn_name, &sig);
        let func_ref = self.module.declare_func_in_func(id, &mut b.func);

        let call = b.ins().call(func_ref, &[tv.val]);
        let val = b.inst_results(call)[0];

        Ok(TypedValue { val, tag: tv.tag })
    }

    fn emit_exit(&mut self, b: &mut FunctionBuilder, arg: Value) {
        let mut sig = self.module.make_signature();

        sig.params.push(AbiParam::new(types::I64));

        let id = self.import("exit", &sig);
        let func_ref = self.module.declare_func_in_func(id, &mut b.func);

        b.ins().call(func_ref, &[arg]);
    }

    fn detect_exit(
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

    fn compile_call(
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

        let func_ref = self.module.declare_func_in_func(id, &mut b.func);
        let call = b.ins().call(func_ref, &compiled);
        let results = b.inst_results(call);

        let val = if results.is_empty() {
            b.ins().iconst(types::I64, 0)
        } else {
            results[0]
        };

        Ok(TypedValue { val, tag: Tag::Int })
    }

    fn store_string(&mut self, b: &mut FunctionBuilder, s: &str) -> Value {
        let mut bytes = s.as_bytes().to_vec();

        bytes.push(0);

        let id = self.module.declare_anonymous_data(true, false).unwrap();

        let mut ctx = DataDescription::new();

        ctx.define(bytes.into());

        self.module.define_data(id, &ctx).unwrap();

        let gv = self.module.declare_data_in_func(id, &mut b.func);

        b.ins().symbol_value(types::I64, gv)
    }

    fn import(&mut self, name: &str, sig: &Signature) -> FuncId {
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

    fn convert_type(&self, b: &mut FunctionBuilder, tv: TypedValue, target: &FllufType) -> Value {
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
}

fn type_to_ir(t: &FllufType) -> Type {
    match t {
        FllufType::Int => types::I64,
        FllufType::Float => types::F64,
        FllufType::Void => types::I64,
    }
}

fn return_type_to_ir(t: &FllufType) -> Option<Type> {
    match t {
        FllufType::Int => Some(types::I64),
        FllufType::Float => Some(types::F64),
        FllufType::Void => None,
    }
}

fn widen(b: &mut FunctionBuilder, v: Value) -> Value {
    if b.func.dfg.value_type(v) == types::I64 {
        b.ins().fcvt_from_sint(types::F64, v)
    } else {
        v
    }
}

fn int_cc(op: BinOp) -> IntCC {
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

fn float_cc(op: BinOp) -> FloatCC {
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

fn select_int(b: &mut FunctionBuilder, cond: Value) -> TypedValue {
    let one = b.ins().iconst(types::I64, 1);
    let zero = b.ins().iconst(types::I64, 0);

    TypedValue {
        val: b.ins().select(cond, one, zero),
        tag: Tag::Int,
    }
}
