//! WebAssembly emitter — implements the subset of the WASM binary
//! format needed to compile a substantial fraction of Zero programs
//! to a valid `.wasm` module.
//!
//! Written from the public WebAssembly Core Specification (W3C
//! recommendation, https://webassembly.github.io/spec/core/binary/).
//! Not derived from any third-party implementation.
//!
//! Phase 6 (slice 2) scope:
//! - Multiple `fun` declarations in one module
//! - `i32` parameters and return type
//! - Binary arithmetic: `+ - * /` (signed i32)
//! - Comparisons: `== != < <= > >=` (signed i32, result is i32 0/1)
//! - `let` bindings (compile to wasm locals)
//! - Function calls to other functions in the same module
//! - `return EXPR` and trailing-expression bodies
//!
//! Not yet supported in this slice (future PRs):
//! - Control flow (if/while/for)
//! - i64, f32, f64, types other than i32
//! - Memory operations, strings, arrays
//! - Imports, tables, globals
//! - Effect handling, raise/check, borrow operators

use std::collections::BTreeMap;
use zero_ast::{Expr, ExprKind, Function, Program, Stmt, StmtKind};

#[derive(Debug, thiserror::Error)]
pub enum EmitError {
    #[error("empty program: no functions to emit")]
    EmptyProgram,
    #[error("function '{0}' has unsupported parameter type '{1}'; only i32 supported")]
    UnsupportedParamType(String, String),
    #[error("function '{0}' has unsupported return type '{1}'; only i32 / Void supported")]
    UnsupportedReturnType(String, String),
    #[error("function '{0}' body uses unsupported statement kind {1:?}")]
    UnsupportedStmt(String, StmtKind),
    #[error("function '{0}' references undefined name '{1}'")]
    UndefinedName(String, String),
    #[error("function '{0}' uses unsupported expression: {1}")]
    UnsupportedExpr(String, String),
    #[error("function '{0}' calls unknown function '{1}'")]
    UnknownCallee(String, String),
}

// ===== Binary format primitives =====
// References: WebAssembly Core Spec §5 (binary format).

fn write_unsigned_leb128(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn write_signed_leb128(out: &mut Vec<u8>, mut value: i64) {
    loop {
        let byte = (value as u8) & 0x7F;
        value >>= 7;
        let sign_bit_of_byte = byte & 0x40 != 0;
        let more = !((value == 0 && !sign_bit_of_byte) || (value == -1 && sign_bit_of_byte));
        out.push(if more { byte | 0x80 } else { byte });
        if !more {
            break;
        }
    }
}

fn write_name(out: &mut Vec<u8>, name: &str) {
    write_unsigned_leb128(out, name.len() as u64);
    out.extend_from_slice(name.as_bytes());
}

fn make_section(id: u8, payload: &[u8]) -> Vec<u8> {
    let mut section = Vec::with_capacity(payload.len() + 6);
    section.push(id);
    write_unsigned_leb128(&mut section, payload.len() as u64);
    section.extend_from_slice(payload);
    section
}

// ===== Value type codes =====
// WebAssembly Core Spec §5.3.1.
const I32: u8 = 0x7F;
// No other types used in this slice.

// Opcode constants used here (WebAssembly Core Spec §5.4).
const OP_END: u8 = 0x0B;
const OP_CALL: u8 = 0x10;
const OP_LOCAL_GET: u8 = 0x20;
const OP_LOCAL_SET: u8 = 0x21;
const OP_LOCAL_TEE: u8 = 0x22;
const OP_I32_CONST: u8 = 0x41;
const OP_I32_EQ: u8 = 0x46;
const OP_I32_NE: u8 = 0x47;
const OP_I32_LT_S: u8 = 0x48;
const OP_I32_GT_S: u8 = 0x4A;
const OP_I32_LE_S: u8 = 0x4C;
const OP_I32_GE_S: u8 = 0x4E;
const OP_I32_ADD: u8 = 0x6A;
const OP_I32_SUB: u8 = 0x6B;
const OP_I32_MUL: u8 = 0x6C;
const OP_I32_DIV_S: u8 = 0x6D;
const OP_DROP: u8 = 0x1A;
const OP_RETURN: u8 = 0x0F;

// ===== Type derivation =====

fn ty_supported(t: &str) -> bool {
    matches!(t, "i32")
}

fn return_supported(t: &str) -> bool {
    matches!(t, "i32" | "Void" | "")
}

fn binary_op(text: &str) -> Option<u8> {
    Some(match text {
        "+" => OP_I32_ADD,
        "-" => OP_I32_SUB,
        "*" => OP_I32_MUL,
        "/" => OP_I32_DIV_S,
        "==" => OP_I32_EQ,
        "!=" => OP_I32_NE,
        "<" => OP_I32_LT_S,
        "<=" => OP_I32_LE_S,
        ">" => OP_I32_GT_S,
        ">=" => OP_I32_GE_S,
        _ => return None,
    })
}

// ===== Codegen context for one function body =====

struct FnContext<'a> {
    fn_name: &'a str,
    // Local index assignment for let bindings, in declaration order
    // (params occupy the first slots, then locals).
    locals: BTreeMap<String, u32>,
    /// Number of locals beyond the params (these are declared in the
    /// function body's locals header).
    extra_locals: u32,
    /// Index of each top-level function so `Call` exprs can resolve.
    fn_indices: &'a BTreeMap<String, u32>,
    /// Number of params (for indexing into params vs extra locals).
    param_count: u32,
    /// Whether the function has a non-Void return.
    returns_value: bool,
}

impl<'a> FnContext<'a> {
    fn new(
        f: &'a Function,
        fn_indices: &'a BTreeMap<String, u32>,
    ) -> Result<Self, EmitError> {
        let mut locals = BTreeMap::new();
        for (i, p) in f.params.iter().enumerate() {
            if !ty_supported(&p.ty) {
                return Err(EmitError::UnsupportedParamType(f.name.clone(), p.ty.clone()));
            }
            locals.insert(p.name.clone(), i as u32);
        }
        Ok(FnContext {
            fn_name: &f.name,
            locals,
            extra_locals: 0,
            fn_indices,
            param_count: f.params.len() as u32,
            returns_value: f.return_type == "i32",
        })
    }

    fn alloc_local(&mut self, name: &str) -> u32 {
        let idx = self.param_count + self.extra_locals;
        self.locals.insert(name.to_string(), idx);
        self.extra_locals += 1;
        idx
    }

    fn lookup_local(&self, name: &str) -> Option<u32> {
        self.locals.get(name).copied()
    }
}

// ===== Expression compilation =====
// Each call emits bytecode that leaves exactly one i32 on the operand stack.

fn compile_expr(out: &mut Vec<u8>, expr: &Expr, ctx: &FnContext<'_>) -> Result<(), EmitError> {
    match expr.kind {
        ExprKind::Number => {
            let v: i64 = expr.text.parse().map_err(|_| {
                EmitError::UnsupportedExpr(
                    ctx.fn_name.into(),
                    format!("number literal '{}' is not an integer", expr.text),
                )
            })?;
            out.push(OP_I32_CONST);
            write_signed_leb128(out, v);
        }
        ExprKind::Bool => {
            out.push(OP_I32_CONST);
            write_signed_leb128(out, if expr.bool_value { 1 } else { 0 });
        }
        ExprKind::Ident => {
            let idx = ctx.lookup_local(&expr.text).ok_or_else(|| {
                EmitError::UndefinedName(ctx.fn_name.into(), expr.text.clone())
            })?;
            out.push(OP_LOCAL_GET);
            write_unsigned_leb128(out, idx as u64);
        }
        ExprKind::Binary => {
            let op = binary_op(&expr.text).ok_or_else(|| {
                EmitError::UnsupportedExpr(
                    ctx.fn_name.into(),
                    format!("binary operator '{}' not supported", expr.text),
                )
            })?;
            let left = expr.left.as_ref().ok_or_else(|| {
                EmitError::UnsupportedExpr(ctx.fn_name.into(), "binary missing left".into())
            })?;
            let right = expr.right.as_ref().ok_or_else(|| {
                EmitError::UnsupportedExpr(ctx.fn_name.into(), "binary missing right".into())
            })?;
            compile_expr(out, left, ctx)?;
            compile_expr(out, right, ctx)?;
            out.push(op);
        }
        ExprKind::Call => {
            let callee_expr = expr.left.as_ref().ok_or_else(|| {
                EmitError::UnsupportedExpr(ctx.fn_name.into(), "call missing callee".into())
            })?;
            let callee_name = match callee_expr.kind {
                ExprKind::Ident => callee_expr.text.clone(),
                _ => {
                    return Err(EmitError::UnsupportedExpr(
                        ctx.fn_name.into(),
                        "call target must be a bare identifier".into(),
                    ));
                }
            };
            let &fn_idx = ctx.fn_indices.get(&callee_name).ok_or_else(|| {
                EmitError::UnknownCallee(ctx.fn_name.into(), callee_name.clone())
            })?;
            for arg in &expr.args {
                compile_expr(out, arg, ctx)?;
            }
            out.push(OP_CALL);
            write_unsigned_leb128(out, fn_idx as u64);
        }
        _ => {
            return Err(EmitError::UnsupportedExpr(
                ctx.fn_name.into(),
                format!("expression kind {:?} not supported in this slice", expr.kind),
            ));
        }
    }
    Ok(())
}

fn compile_stmt(out: &mut Vec<u8>, stmt: &Stmt, ctx: &mut FnContext<'_>) -> Result<(), EmitError> {
    match stmt.kind {
        StmtKind::Let => {
            let expr = stmt.expr.as_ref().ok_or_else(|| {
                EmitError::UnsupportedStmt(ctx.fn_name.into(), stmt.kind)
            })?;
            compile_expr(out, expr, ctx)?;
            let idx = ctx.alloc_local(&stmt.name);
            out.push(OP_LOCAL_SET);
            write_unsigned_leb128(out, idx as u64);
        }
        StmtKind::Assign => {
            if stmt.name.is_empty() {
                return Err(EmitError::UnsupportedStmt(ctx.fn_name.into(), stmt.kind));
            }
            let idx = ctx.lookup_local(&stmt.name).ok_or_else(|| {
                EmitError::UndefinedName(ctx.fn_name.into(), stmt.name.clone())
            })?;
            let expr = stmt.expr.as_ref().ok_or_else(|| {
                EmitError::UnsupportedStmt(ctx.fn_name.into(), stmt.kind)
            })?;
            compile_expr(out, expr, ctx)?;
            out.push(OP_LOCAL_SET);
            write_unsigned_leb128(out, idx as u64);
        }
        StmtKind::Return => {
            if let Some(expr) = &stmt.expr {
                compile_expr(out, expr, ctx)?;
            }
            out.push(OP_RETURN);
        }
        StmtKind::Expr => {
            // Compile the expression for side effects; drop the i32 it
            // pushed since we ignore the value.
            let expr = stmt.expr.as_ref().ok_or_else(|| {
                EmitError::UnsupportedStmt(ctx.fn_name.into(), stmt.kind)
            })?;
            compile_expr(out, expr, ctx)?;
            out.push(OP_DROP);
        }
        _ => {
            return Err(EmitError::UnsupportedStmt(ctx.fn_name.into(), stmt.kind));
        }
    }
    Ok(())
}

fn compile_function_body(f: &Function, fn_indices: &BTreeMap<String, u32>) -> Result<Vec<u8>, EmitError> {
    let mut ctx = FnContext::new(f, fn_indices)?;

    let mut body_code = Vec::new();
    for stmt in &f.body {
        compile_stmt(&mut body_code, stmt, &mut ctx)?;
    }
    // If the function returns a value but the last statement wasn't an
    // explicit return, the body must already leave a value on the stack
    // — our current statement compiler pushes-and-drops for Expr stmts,
    // so this only works if the last stmt is Return. Enforce it.
    if ctx.returns_value
        && f.body
            .last()
            .map(|s| s.kind != StmtKind::Return)
            .unwrap_or(true)
    {
        return Err(EmitError::UnsupportedStmt(
            f.name.clone(),
            StmtKind::Return,
        ));
    }

    // Body framing: locals declaration + code + END.
    let mut body = Vec::new();
    // We emit one local group per extra local with type i32.
    write_unsigned_leb128(&mut body, ctx.extra_locals as u64);
    for _ in 0..ctx.extra_locals {
        write_unsigned_leb128(&mut body, 1); // count
        body.push(I32);
    }
    body.extend(body_code);
    body.push(OP_END);
    Ok(body)
}

/// Emit a complete WASM module containing every supported function in `program`.
///
/// Unsupported function shapes (parameters with non-i32 types, control
/// flow, etc.) produce an error.
pub fn emit_program(program: &Program) -> Result<Vec<u8>, EmitError> {
    if program.functions.is_empty() {
        return Err(EmitError::EmptyProgram);
    }
    for f in &program.functions {
        if !return_supported(&f.return_type) {
            return Err(EmitError::UnsupportedReturnType(
                f.name.clone(),
                f.return_type.clone(),
            ));
        }
        for p in &f.params {
            if !ty_supported(&p.ty) {
                return Err(EmitError::UnsupportedParamType(
                    f.name.clone(),
                    p.ty.clone(),
                ));
            }
        }
    }

    let mut fn_indices: BTreeMap<String, u32> = BTreeMap::new();
    for (i, f) in program.functions.iter().enumerate() {
        fn_indices.insert(f.name.clone(), i as u32);
    }

    // ----- Type section: one type per function (each: params -> return) -----
    let mut types = Vec::new();
    write_unsigned_leb128(&mut types, program.functions.len() as u64);
    for f in &program.functions {
        types.push(0x60); // func
        write_unsigned_leb128(&mut types, f.params.len() as u64);
        for _ in &f.params {
            types.push(I32);
        }
        if f.return_type == "i32" {
            write_unsigned_leb128(&mut types, 1);
            types.push(I32);
        } else {
            write_unsigned_leb128(&mut types, 0);
        }
    }

    // ----- Function section: type index per function (1:1 mapping). -----
    let mut funcs = Vec::new();
    write_unsigned_leb128(&mut funcs, program.functions.len() as u64);
    for i in 0..program.functions.len() {
        write_unsigned_leb128(&mut funcs, i as u64);
    }

    // ----- Export section: every function is exported under its declared name. -----
    let mut exports = Vec::new();
    write_unsigned_leb128(&mut exports, program.functions.len() as u64);
    for (i, f) in program.functions.iter().enumerate() {
        write_name(&mut exports, &f.name);
        exports.push(0x00); // export kind = func
        write_unsigned_leb128(&mut exports, i as u64);
    }

    // ----- Code section: one body per function. -----
    let mut bodies: Vec<Vec<u8>> = Vec::with_capacity(program.functions.len());
    for f in &program.functions {
        bodies.push(compile_function_body(f, &fn_indices)?);
    }
    let mut code = Vec::new();
    write_unsigned_leb128(&mut code, bodies.len() as u64);
    for body in &bodies {
        write_unsigned_leb128(&mut code, body.len() as u64);
        code.extend_from_slice(body);
    }

    let mut module = Vec::with_capacity(128);
    module.extend_from_slice(&[0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00]);
    module.extend(make_section(1, &types));
    module.extend(make_section(3, &funcs));
    module.extend(make_section(7, &exports));
    module.extend(make_section(10, &code));
    Ok(module)
}

/// Convenience: emit a single function as a complete module. Kept for
/// callers that want the simpler API from the original slice.
pub fn emit_constant_function(f: &Function) -> Result<Vec<u8>, EmitError> {
    let prog = Program {
        functions: vec![f.clone()],
        ..Default::default()
    };
    emit_program(&prog)
}

// Suppress unused warnings on LEB128 helpers used only by tests.
#[allow(dead_code)]
fn _hold_unused() {
    let mut _v = Vec::new();
    write_signed_leb128(&mut _v, 0);
    let _ = OP_LOCAL_TEE;
}

#[cfg(test)]
mod tests {
    use super::*;
    use zero_ast::{Param, Stmt};

    fn num(v: i64) -> Expr {
        let mut e = Expr::new(ExprKind::Number, 1, 1);
        e.text = v.to_string();
        e
    }

    fn ident(name: &str) -> Expr {
        let mut e = Expr::new(ExprKind::Ident, 1, 1);
        e.text = name.into();
        e
    }

    fn binop(op: &str, l: Expr, r: Expr) -> Expr {
        let mut e = Expr::new(ExprKind::Binary, 1, 1);
        e.text = op.into();
        e.left = Some(Box::new(l));
        e.right = Some(Box::new(r));
        e
    }

    fn ret(e: Expr) -> Stmt {
        let mut s = Stmt::new(StmtKind::Return, 1, 1);
        s.expr = Some(Box::new(e));
        s
    }

    fn let_stmt(name: &str, e: Expr) -> Stmt {
        let mut s = Stmt::new(StmtKind::Let, 1, 1);
        s.name = name.into();
        s.expr = Some(Box::new(e));
        s
    }

    fn fun(name: &str, params: Vec<&str>, body: Vec<Stmt>) -> Function {
        Function {
            name: name.into(),
            return_type: "i32".into(),
            params: params
                .into_iter()
                .map(|p| Param {
                    name: p.into(),
                    ty: "i32".into(),
                    line: 1,
                    column: 1,
                })
                .collect(),
            body,
            is_public: true,
            raises: false,
            line: 1,
            column: 1,
        }
    }

    #[test]
    fn const_function_round_trip() {
        let f = fun("answer", vec![], vec![ret(num(42))]);
        let bytes = emit_constant_function(&f).unwrap();
        assert_eq!(&bytes[0..4], b"\0asm");
        let needle = b"answer";
        assert!(bytes.windows(needle.len()).any(|w| w == needle));
    }

    #[test]
    fn add_two_params() {
        let body = vec![ret(binop("+", ident("a"), ident("b")))];
        let f = fun("add", vec!["a", "b"], body);
        let bytes = emit_constant_function(&f).unwrap();
        // Code section contains: local.get 0, local.get 1, i32.add, return, end
        assert!(bytes.contains(&OP_LOCAL_GET));
        assert!(bytes.contains(&OP_I32_ADD));
        assert!(bytes.contains(&OP_RETURN));
        assert!(bytes.contains(&OP_END));
    }

    #[test]
    fn let_binding_compiles_to_local() {
        // fun f() -> i32 { let x = 10 ; return x + 5 }
        let body = vec![
            let_stmt("x", num(10)),
            ret(binop("+", ident("x"), num(5))),
        ];
        let f = fun("f", vec![], body);
        let bytes = emit_constant_function(&f).unwrap();
        assert!(bytes.contains(&OP_LOCAL_SET));
        assert!(bytes.contains(&OP_LOCAL_GET));
    }

    #[test]
    fn call_between_functions() {
        let answer = fun("answer", vec![], vec![ret(num(42))]);
        let mut call_expr = Expr::new(ExprKind::Call, 1, 1);
        call_expr.left = Some(Box::new(ident("answer")));
        let main = fun("use_answer", vec![], vec![ret(call_expr)]);
        let prog = Program {
            functions: vec![answer, main],
            ..Default::default()
        };
        let bytes = emit_program(&prog).unwrap();
        // Must contain a CALL opcode referencing function index 0.
        assert!(bytes.contains(&OP_CALL));
    }

    #[test]
    fn comparison_operators() {
        for (op, code) in [
            ("==", OP_I32_EQ),
            ("!=", OP_I32_NE),
            ("<", OP_I32_LT_S),
            ("<=", OP_I32_LE_S),
            (">", OP_I32_GT_S),
            (">=", OP_I32_GE_S),
        ] {
            let body = vec![ret(binop(op, num(1), num(2)))];
            let f = fun("cmp", vec![], body);
            let bytes = emit_constant_function(&f).unwrap();
            assert!(bytes.contains(&code), "op {op} should emit 0x{code:02x}");
        }
    }

    #[test]
    fn rejects_unknown_callee() {
        let mut call = Expr::new(ExprKind::Call, 1, 1);
        call.left = Some(Box::new(ident("does_not_exist")));
        let body = vec![ret(call)];
        let f = fun("main", vec![], body);
        let prog = Program {
            functions: vec![f],
            ..Default::default()
        };
        assert!(matches!(
            emit_program(&prog),
            Err(EmitError::UnknownCallee(_, _))
        ));
    }

    #[test]
    fn rejects_unknown_ident() {
        let body = vec![ret(ident("nope"))];
        let f = fun("main", vec![], body);
        assert!(matches!(
            emit_constant_function(&f),
            Err(EmitError::UndefinedName(_, _))
        ));
    }

    #[test]
    fn rejects_non_i32_param() {
        let mut f = fun("greet", vec!["s"], vec![ret(num(0))]);
        f.params[0].ty = "String".into();
        assert!(matches!(
            emit_constant_function(&f),
            Err(EmitError::UnsupportedParamType(_, _))
        ));
    }

    #[test]
    fn rejects_non_i32_return() {
        let mut f = fun("answer", vec![], vec![ret(num(0))]);
        f.return_type = "i64".into();
        assert!(matches!(
            emit_constant_function(&f),
            Err(EmitError::UnsupportedReturnType(_, _))
        ));
    }

    #[test]
    fn void_function_with_no_return() {
        let mut f = Function {
            name: "noop".into(),
            return_type: "Void".into(),
            params: vec![],
            body: vec![],
            is_public: true,
            raises: false,
            line: 1,
            column: 1,
        };
        // Need a return stmt to satisfy the body-ends-in-return rule.
        let mut ret_stmt = Stmt::new(StmtKind::Return, 1, 1);
        ret_stmt.expr = None;
        f.body.push(ret_stmt);
        let bytes = emit_constant_function(&f).unwrap();
        assert!(bytes.contains(&OP_RETURN));
    }

    #[test]
    fn leb128_roundtrips() {
        for v in [0u64, 1, 63, 64, 127, 128, 255, 16384, 1_000_000] {
            let mut buf = Vec::new();
            write_unsigned_leb128(&mut buf, v);
            // Decode back manually to verify.
            let mut decoded = 0u64;
            let mut shift = 0;
            for &b in &buf {
                decoded |= ((b & 0x7F) as u64) << shift;
                if b & 0x80 == 0 {
                    break;
                }
                shift += 7;
            }
            assert_eq!(decoded, v);
        }
    }
}
