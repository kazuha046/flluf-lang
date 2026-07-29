use std::collections::{HashMap, HashSet};

use cranelift::codegen::ir::{entities::StackSlot, types};
use cranelift::prelude::*;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{DataDescription, FuncId, Linkage, Module};

use crate::ast::*;

type VarMap = HashMap<String, (StackSlot, FllufTypeTag)>;

#[derive(Clone, Copy, Debug, PartialEq)]
enum FllufTypeTag {
    Int,
    Float,
    Pointer,
}

struct TypedValue {
    value: Value,
    typ: FllufTypeTag,
}

pub fn compile(program: &Program) -> Vec<u8> {
    let isa_builder = cranelift_native::builder()
        .unwrap_or_else(|_| isa::lookup("x86-64".parse().unwrap()).unwrap());

    let flags = settings::Flags::new(settings::builder());
    let isa = isa_builder.finish(flags).unwrap();

    let obj_builder = cranelift_object::ObjectBuilder::new(
        isa,
        "flluf_compiled",
        cranelift_module::default_libcall_names(),
    )
    .unwrap();

    let module = cranelift_object::ObjectModule::new(obj_builder);

    let mut compiler = Compiler {
        module,
        imports: HashSet::new(),
    };

    compiler.compile_program(program);

    let product = compiler.module.finish();

    product.emit().unwrap()
}

struct Compiler {
    module: cranelift_object::ObjectModule,
    imports: HashSet<String>,
}

impl Compiler {
    fn compile_program(&mut self, program: &Program) {
        // Pass 1: declare all functions with their proper signatures
        let func_ids: Vec<_> = program
            .functions
            .iter()
            .map(|func| self.declare_function(func))
            .collect();

        // Pass 2: compile function bodies
        let mut ctx = FunctionBuilderContext::new();

        for (func, func_id) in program.functions.iter().zip(func_ids.iter()) {
            self.compile_function_body(func, *func_id, &mut ctx, program.use_system);
        }
    }

    fn declare_function(&mut self, func: &Function) -> FuncId {
        let mut sig = self.module.make_signature();

        for param in &func.params {
            let ty = flluf_type_to_ir(&param.param_type);
            sig.params.push(AbiParam::new(ty));
        }

        if func.name == "Main" {
            sig.returns.push(AbiParam::new(types::I64));
        } else if let Some(ty) = flluf_type_to_ir_opt(&func.return_type) {
            sig.returns.push(AbiParam::new(ty));
        }

        let entry_name = if func.name == "Main" {
            "main"
        } else {
            &func.name
        };

        self.module
            .declare_function(entry_name, Linkage::Export, &sig)
            .unwrap()
    }

    fn compile_function_body(
        &mut self,
        func: &Function,
        func_id: FuncId,
        ctx: &mut FunctionBuilderContext,
        use_system: bool,
    ) {
        let mut func_data = self.module.make_context();
        let sig = self
            .module
            .declarations()
            .get_function_decl(func_id)
            .signature
            .clone();

        func_data.func.signature = sig;

        let mut builder = FunctionBuilder::new(&mut func_data.func, ctx);
        let entry_block = builder.create_block();

        builder.append_block_params_for_function_params(entry_block);
        builder.switch_to_block(entry_block);
        builder.seal_block(entry_block);

        let mut vars: VarMap = HashMap::new();

        let param_values = builder.block_params(entry_block).to_vec();

        for (i, param) in func.params.iter().enumerate() {
            let ty = flluf_type_to_ir(&param.param_type);

            let slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                ty.bytes(),
                0,
            ));

            builder.ins().stack_store(param_values[i], slot, 0);

            let vt = match &param.param_type {
                FllufType::Int => FllufTypeTag::Int,
                FllufType::Float => FllufTypeTag::Float,
                _ => FllufTypeTag::Int,
            };

            vars.insert(param.name.clone(), (slot, vt));
        }

        for stmt in &func.body.statements {
            self.compile_stmt(&mut builder, &mut vars, stmt, use_system);
        }

        if func.return_type == FllufType::Void && func.name != "Main" {
            builder.ins().return_(&[]);
        } else if func.name == "Main" {
            let zero = builder.ins().iconst(types::I64, 0);
            builder.ins().return_(&[zero]);
        }

        builder.finalize();

        self.module
            .define_function(func_id, &mut func_data)
            .unwrap();
    }

    fn compile_stmt(
        &mut self,
        builder: &mut FunctionBuilder,
        vars: &mut VarMap,
        stmt: &Stmt,
        use_system: bool,
    ) {
        match stmt {
            Stmt::VarDecl {
                var_type,
                name,
                value,
            } => {
                let ir_ty = flluf_type_to_ir(var_type);

                let slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    ir_ty.bytes(),
                    0,
                ));

                let tv = self.compile_expr(builder, vars, value, use_system);
                let val = self.convert_to(builder, tv, var_type);

                builder.ins().stack_store(val, slot, 0);

                let vt = match var_type {
                    FllufType::Int => FllufTypeTag::Int,
                    FllufType::Float => FllufTypeTag::Float,

                    _ => FllufTypeTag::Int,
                };

                vars.insert(name.clone(), (slot, vt));
            }

            Stmt::Return(expr) => {
                if is_exit_call(expr) {
                    let arg = get_exit_arg(expr);
                    let tv = self.compile_expr(builder, vars, arg, use_system);

                    self.call_exit(builder, tv.value);
                } else {
                    let tv = self.compile_expr(builder, vars, expr, use_system);
                    builder.ins().return_(&[tv.value]);
                }
            }

            Stmt::Expr(expr) => {
                if is_exit_call(expr) {
                    let arg = get_exit_arg(expr);
                    let tv = self.compile_expr(builder, vars, arg, use_system);

                    self.call_exit(builder, tv.value);
                } else {
                    self.compile_expr(builder, vars, expr, use_system);
                }
            }

            Stmt::If {
                cond,
                then_block,
                else_ifs,
                else_block,
            } => {
                let end_block = builder.create_block();

                let tv = self.compile_expr(builder, vars, cond, use_system);
                let zero = builder.ins().iconst(types::I64, 0);
                let is_true = builder.ins().icmp(IntCC::NotEqual, tv.value, zero);

                let then_block_0 = builder.create_block();
                let else_chain = builder.create_block();

                builder
                    .ins()
                    .brif(is_true, then_block_0, &[], else_chain, &[]);

                builder.switch_to_block(then_block_0);
                builder.seal_block(then_block_0);

                for s in &then_block.statements {
                    self.compile_stmt(builder, vars, s, use_system);
                }

                builder.ins().jump(end_block, &[]);

                let mut current_else = else_chain;

                for (ei_cond, ei_body) in else_ifs {
                    builder.switch_to_block(current_else);
                    builder.seal_block(current_else);

                    let tv_cond = self.compile_expr(builder, vars, ei_cond, use_system);
                    let z = builder.ins().iconst(types::I64, 0);
                    let is_true = builder.ins().icmp(IntCC::NotEqual, tv_cond.value, z);

                    let then_block_n = builder.create_block();
                    let next_else = builder.create_block();

                    builder
                        .ins()
                        .brif(is_true, then_block_n, &[], next_else, &[]);

                    builder.switch_to_block(then_block_n);
                    builder.seal_block(then_block_n);

                    for s in &ei_body.statements {
                        self.compile_stmt(builder, vars, s, use_system);
                    }

                    builder.ins().jump(end_block, &[]);

                    current_else = next_else;
                }

                builder.switch_to_block(current_else);
                builder.seal_block(current_else);

                if let Some(eb) = else_block {
                    for s in &eb.statements {
                        self.compile_stmt(builder, vars, s, use_system);
                    }
                }

                builder.ins().jump(end_block, &[]);

                builder.switch_to_block(end_block);
                builder.seal_block(end_block);
            }
        }
    }

    fn convert_to(
        &self,
        builder: &mut FunctionBuilder,
        tv: TypedValue,
        target: &FllufType,
    ) -> Value {
        let ir_target = flluf_type_to_ir(target);

        if builder.func.dfg.value_type(tv.value) == ir_target {
            return tv.value;
        }

        match (tv.typ, target) {
            (FllufTypeTag::Int, FllufType::Float) => {
                builder.ins().fcvt_from_sint(types::F64, tv.value)
            }
            (FllufTypeTag::Float, FllufType::Int) => {
                builder.ins().fcvt_to_sint(types::I64, tv.value)
            }

            _ => tv.value,
        }
    }

    fn compile_expr(
        &mut self,
        builder: &mut FunctionBuilder,
        vars: &mut VarMap,
        expr: &Expr,
        use_system: bool,
    ) -> TypedValue {
        match expr {
            Expr::IntLit(n) => TypedValue {
                value: builder.ins().iconst(types::I64, *n),
                typ: FllufTypeTag::Int,
            },

            Expr::FloatLit(n) => {
                let bits = n.to_bits() as i64;
                let tmp = builder.ins().iconst(types::I64, bits);
                let val = builder.ins().bitcast(types::F64, MemFlags::new(), tmp);

                TypedValue {
                    value: val,
                    typ: FllufTypeTag::Float,
                }
            }

            Expr::StringLit(s) => TypedValue {
                value: self.emit_string_data(builder, s),
                typ: FllufTypeTag::Pointer,
            },

            Expr::Variable(name) => {
                let (slot, vt) = vars.get(name.as_str()).expect("undefined variable");

                let ty = match vt {
                    FllufTypeTag::Int => types::I64,
                    FllufTypeTag::Float => types::F64,
                    FllufTypeTag::Pointer => types::I64,
                };

                let val = builder.ins().stack_load(ty, *slot, 0);

                TypedValue {
                    value: val,
                    typ: *vt,
                }
            }

            Expr::BinaryOp(left, op, right) => {
                self.compile_binary(builder, vars, left, *op, right, use_system)
            }

            Expr::Call(name, args) if name == "Exit" => {
                let tv = self.compile_expr(builder, vars, &args[0], use_system);

                self.call_exit(builder, tv.value);

                TypedValue {
                    value: builder.ins().iconst(types::I64, 0),
                    typ: FllufTypeTag::Int,
                }
            }

            Expr::Call(name, args) if name == "Log" => {
                self.compile_log(builder, vars, args, use_system)
            }

            Expr::Call(name, args) => self.compile_user_call(builder, vars, name, args, use_system),

            Expr::ModuleCall(mod_name, func_name, args) => {
                if mod_name == "System" && func_name == "Exit" {
                    let tv = self.compile_expr(builder, vars, &args[0], use_system);

                    self.call_exit(builder, tv.value);

                    TypedValue {
                        value: builder.ins().iconst(types::I64, 0),
                        typ: FllufTypeTag::Int,
                    }
                } else if mod_name == "System" && func_name == "Log" {
                    self.compile_log(builder, vars, args, use_system)
                } else {
                    panic!("unknown module function: {mod_name}::{func_name}");
                }
            }
        }
    }

    fn compile_binary(
        &mut self,
        builder: &mut FunctionBuilder,
        vars: &mut VarMap,
        left: &Expr,
        op: BinOp,
        right: &Expr,
        use_system: bool,
    ) -> TypedValue {
        let l = self.compile_expr(builder, vars, left, use_system);
        let r = self.compile_expr(builder, vars, right, use_system);

        let l_val = l.value;
        let r_val = r.value;
        let l_ty = builder.func.dfg.value_type(l_val);
        let r_ty = builder.func.dfg.value_type(r_val);

        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
                let is_float = l.typ == FllufTypeTag::Float || r.typ == FllufTypeTag::Float;

                if is_float {
                    let lf = if l_ty == types::I64 {
                        builder.ins().fcvt_from_sint(types::F64, l_val)
                    } else {
                        l_val
                    };

                    let rf = if r_ty == types::I64 {
                        builder.ins().fcvt_from_sint(types::F64, r_val)
                    } else {
                        r_val
                    };

                    let val = match op {
                        BinOp::Add => builder.ins().fadd(lf, rf),
                        BinOp::Sub => builder.ins().fsub(lf, rf),
                        BinOp::Mul => builder.ins().fmul(lf, rf),
                        BinOp::Div => builder.ins().fdiv(lf, rf),

                        _ => unreachable!(),
                    };

                    TypedValue {
                        value: val,
                        typ: FllufTypeTag::Float,
                    }
                } else {
                    let val = match op {
                        BinOp::Add => builder.ins().iadd(l_val, r_val),
                        BinOp::Sub => builder.ins().isub(l_val, r_val),
                        BinOp::Mul => builder.ins().imul(l_val, r_val),
                        BinOp::Div => builder.ins().sdiv(l_val, r_val),

                        _ => unreachable!(),
                    };
                    TypedValue {
                        value: val,
                        typ: FllufTypeTag::Int,
                    }
                }
            }

            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
                let is_float = l.typ == FllufTypeTag::Float || r.typ == FllufTypeTag::Float;

                let cond = if !is_float {
                    let cc = match op {
                        BinOp::Eq => IntCC::Equal,
                        BinOp::Ne => IntCC::NotEqual,
                        BinOp::Lt => IntCC::SignedLessThan,
                        BinOp::Gt => IntCC::SignedGreaterThan,
                        BinOp::Le => IntCC::SignedLessThanOrEqual,
                        BinOp::Ge => IntCC::SignedGreaterThanOrEqual,

                        _ => unreachable!(),
                    };

                    builder.ins().icmp(cc, l_val, r_val)
                } else {
                    let lf = if l_ty == types::I64 {
                        builder.ins().fcvt_from_sint(types::F64, l_val)
                    } else {
                        l_val
                    };

                    let rf = if r_ty == types::I64 {
                        builder.ins().fcvt_from_sint(types::F64, r_val)
                    } else {
                        r_val
                    };

                    let cc = match op {
                        BinOp::Eq => FloatCC::Equal,
                        BinOp::Ne => FloatCC::NotEqual,
                        BinOp::Lt => FloatCC::LessThan,
                        BinOp::Gt => FloatCC::GreaterThan,
                        BinOp::Le => FloatCC::LessThanOrEqual,
                        BinOp::Ge => FloatCC::GreaterThanOrEqual,

                        _ => unreachable!(),
                    };
                    builder.ins().fcmp(cc, lf, rf)
                };

                let one = builder.ins().iconst(types::I64, 1);
                let zero = builder.ins().iconst(types::I64, 0);
                let val = builder.ins().select(cond, one, zero);

                TypedValue {
                    value: val,
                    typ: FllufTypeTag::Int,
                }
            }
        }
    }

    fn declare_import(&mut self, name: &str, sig: &Signature) -> FuncId {
        if self.imports.insert(name.to_string()) {
            self.module
                .declare_function(name, Linkage::Import, sig)
                .unwrap()
        } else {
            self.module
                .get_name(name)
                .map(|f| match f {
                    cranelift_module::FuncOrDataId::Func(id) => id,
                    _ => panic!("expected function {name}"),
                })
                .expect("function previously declared but not found")
        }
    }

    fn compile_log(
        &mut self,
        builder: &mut FunctionBuilder,
        vars: &mut VarMap,
        args: &[Expr],
        use_system: bool,
    ) -> TypedValue {
        let tv = self.compile_expr(builder, vars, &args[0], use_system);

        let (fn_name, ret_ty) = match tv.typ {
            FllufTypeTag::Float => ("__flluf_log_f64", types::F64),
            FllufTypeTag::Pointer => ("__flluf_log_ptr", types::I64),
            FllufTypeTag::Int => ("__flluf_log_i64", types::I64),
        };

        let mut callee_sig = self.module.make_signature();

        callee_sig
            .params
            .push(AbiParam::new(builder.func.dfg.value_type(tv.value)));

        callee_sig.returns.push(AbiParam::new(ret_ty));

        let callee_id = self.declare_import(fn_name, &callee_sig);

        let func_ref = self
            .module
            .declare_func_in_func(callee_id, &mut builder.func);

        let call = builder.ins().call(func_ref, &[tv.value]);
        let results = builder.inst_results(call);

        TypedValue {
            value: results[0],
            typ: tv.typ,
        }
    }

    fn call_exit(&mut self, builder: &mut FunctionBuilder, arg: Value) {
        let mut exit_sig = self.module.make_signature();

        exit_sig.params.push(AbiParam::new(types::I64));

        let exit_id = self.declare_import("exit", &exit_sig);
        let func_ref = self.module.declare_func_in_func(exit_id, &mut builder.func);

        builder.ins().call(func_ref, &[arg]);
    }

    fn compile_user_call(
        &mut self,
        builder: &mut FunctionBuilder,
        vars: &mut VarMap,
        name: &str,
        args: &[Expr],
        use_system: bool,
    ) -> TypedValue {
        let callee = if name == "Main" { "main" } else { name };

        let (callee_id, expected_params) = if let Some(fod) = self.module.get_name(callee) {
            match fod {
                cranelift_module::FuncOrDataId::Func(id) => {
                    let decl = self.module.declarations().get_function_decl(id);
                    let params: Vec<_> =
                        decl.signature.params.iter().map(|p| p.value_type).collect();

                    (id, params)
                }

                _ => panic!("{callee} is not a function"),
            }
        } else {
            let mut sig = self.module.make_signature();

            for arg in args {
                let tv = self.compile_expr(builder, vars, arg, use_system);
                sig.params
                    .push(AbiParam::new(builder.func.dfg.value_type(tv.value)));
            }

            sig.returns.push(AbiParam::new(types::I64));

            let id = self.declare_import(callee, &sig);
            let params: Vec<_> = sig.params.iter().map(|p| p.value_type).collect();

            (id, params)
        };

        let mut compiled_args = Vec::new();

        for (i, arg) in args.iter().enumerate() {
            let tv = self.compile_expr(builder, vars, arg, use_system);
            let val_ty = builder.func.dfg.value_type(tv.value);
            let expected = expected_params.get(i).copied().unwrap_or(val_ty);

            let converted = if val_ty == types::I64 && expected == types::F64 {
                builder.ins().fcvt_from_sint(types::F64, tv.value)
            } else if val_ty == types::F64 && expected == types::I64 {
                builder.ins().fcvt_to_sint(types::I64, tv.value)
            } else {
                tv.value
            };

            compiled_args.push(converted);
        }

        let func_ref = self
            .module
            .declare_func_in_func(callee_id, &mut builder.func);

        let call = builder.ins().call(func_ref, &compiled_args);
        let results = builder.inst_results(call);

        let val = if results.is_empty() {
            builder.ins().iconst(types::I64, 0)
        } else {
            results[0]
        };

        TypedValue {
            value: val,
            typ: FllufTypeTag::Int,
        }
    }

    fn emit_string_data(&mut self, builder: &mut FunctionBuilder, s: &str) -> Value {
        let mut bytes = s.as_bytes().to_vec();

        bytes.push(0);

        let data_id = self.module.declare_anonymous_data(true, false).unwrap();
        let mut data_ctx = DataDescription::new();

        data_ctx.define(bytes.into());
        self.module.define_data(data_id, &data_ctx).unwrap();

        let gv = self.module.declare_data_in_func(data_id, &mut builder.func);

        builder.ins().symbol_value(types::I64, gv)
    }
}

fn flluf_type_to_ir(ty: &FllufType) -> Type {
    match ty {
        FllufType::Int => types::I64,
        FllufType::Float => types::F64,
        FllufType::Void => types::I64,
    }
}

fn flluf_type_to_ir_opt(ty: &FllufType) -> Option<Type> {
    match ty {
        FllufType::Int => Some(types::I64),
        FllufType::Float => Some(types::F64),
        FllufType::Void => None,
    }
}

fn is_exit_call(expr: &Expr) -> bool {
    matches!(expr, Expr::Call(name, _) if name == "Exit")
        || matches!(expr, Expr::ModuleCall(mod_name, func_name, _) if mod_name == "System" && func_name == "Exit")
}

fn get_exit_arg<'a>(expr: &'a Expr) -> &'a Expr {
    match expr {
        Expr::Call(_, args) | Expr::ModuleCall(_, _, args) => &args[0],
        _ => panic!("expected Exit call"),
    }
}
