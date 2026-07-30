use crate::codegen::{Tag, TypedValue};
use crate::syntax::ast::{BinOp, FllufType};
use cranelift::codegen::ir::types;
use cranelift::prelude::*;
use cranelift_frontend::FunctionBuilder;

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
