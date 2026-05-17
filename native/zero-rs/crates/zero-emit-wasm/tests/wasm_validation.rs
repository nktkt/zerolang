//! External validation of the WASM emitter's output using two
//! independently-maintained reference implementations:
//!
//! - `wasmparser`: structural validator (parses the binary, checks type
//!   well-formedness, control-flow nesting, etc.). Catches "we emit
//!   bytes but they aren't actually a valid WASM module" bugs that the
//!   in-crate opcode-presence tests can't see.
//!
//! - `wasmtime`: a real WebAssembly runtime that compiles and executes
//!   the emitted module. Catches bugs where the bytes parse but the
//!   computation is wrong (returned value differs from expected).
//!
//! Both crates are well-known third-party tooling; the tests here use
//! them as black-box libraries and never look at their source.

use wasmparser::{Validator, WasmFeatures};
use zero_ast::{Expr, ExprKind, Function, Param, Program, Stmt, StmtKind};
use zero_emit_wasm::{emit_constant_function, emit_program};

// ===== Builder helpers (local to tests) =====

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
fn if_stmt(cond: Expr, then_body: Vec<Stmt>, else_body: Vec<Stmt>) -> Stmt {
    let mut s = Stmt::new(StmtKind::If, 1, 1);
    s.expr = Some(Box::new(cond));
    s.then_body = then_body;
    s.else_body = else_body;
    s
}
fn while_stmt(cond: Expr, body: Vec<Stmt>) -> Stmt {
    let mut s = Stmt::new(StmtKind::While, 1, 1);
    s.expr = Some(Box::new(cond));
    s.then_body = body;
    s
}
fn assign(name: &str, rhs: Expr) -> Stmt {
    let mut s = Stmt::new(StmtKind::Assign, 1, 1);
    s.name = name.into();
    s.expr = Some(Box::new(rhs));
    s
}

fn fun(name: &str, params: Vec<&str>, body: Vec<Stmt>, return_type: &str) -> Function {
    Function {
        name: name.into(),
        return_type: return_type.into(),
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

// ===== Reusable validation helper =====

fn validate_with_wasmparser(bytes: &[u8]) {
    // Use the default feature set (covers MVP + commonly-on features).
    // If the emitter accidentally produces module shapes outside this
    // set, the validator surfaces it as an error.
    let mut validator = Validator::new_with_features(WasmFeatures::default());
    validator
        .validate_all(bytes)
        .expect("wasmparser should validate emitter output as a well-formed WASM module");
}

// ===== Tests =====

#[test]
fn const_module_validates() {
    let f = fun("answer", vec![], vec![ret(num(42))], "i32");
    let bytes = emit_constant_function(&f).expect("emit");
    validate_with_wasmparser(&bytes);
}

#[test]
fn arithmetic_module_validates() {
    let f = fun("add", vec!["a", "b"], vec![ret(binop("+", ident("a"), ident("b")))], "i32");
    let bytes = emit_constant_function(&f).expect("emit");
    validate_with_wasmparser(&bytes);
}

#[test]
fn let_binding_module_validates() {
    let body = vec![let_stmt("x", num(10)), ret(binop("+", ident("x"), num(5)))];
    let f = fun("f", vec![], body, "i32");
    let bytes = emit_constant_function(&f).expect("emit");
    validate_with_wasmparser(&bytes);
}

#[test]
fn if_else_module_validates() {
    let then_body = vec![ret(num(1))];
    let else_body = vec![ret(num(2))];
    let body = vec![if_stmt(binop("==", num(1), num(1)), then_body, else_body), ret(num(0))];
    let f = fun("chooses", vec![], body, "i32");
    let bytes = emit_constant_function(&f).expect("emit");
    validate_with_wasmparser(&bytes);
}

#[test]
fn while_with_break_module_validates() {
    let body = vec![
        let_stmt("i", num(0)),
        while_stmt(
            binop("<", ident("i"), num(5)),
            vec![assign("i", binop("+", ident("i"), num(1)))],
        ),
        ret(ident("i")),
    ];
    let f = fun("counter", vec![], body, "i32");
    let bytes = emit_constant_function(&f).expect("emit");
    validate_with_wasmparser(&bytes);
}

#[test]
fn multi_function_module_with_call_validates() {
    let answer = fun("answer", vec![], vec![ret(num(42))], "i32");
    let mut call = Expr::new(ExprKind::Call, 1, 1);
    call.left = Some(Box::new(ident("answer")));
    let use_answer = fun("use_answer", vec![], vec![ret(call)], "i32");
    let prog = Program {
        functions: vec![answer, use_answer],
        ..Default::default()
    };
    let bytes = emit_program(&prog).expect("emit");
    validate_with_wasmparser(&bytes);
}

// ===== wasmtime end-to-end execution tests =====
// These compile the emitted WASM with cranelift and call exported
// functions, asserting computed values match expected.

fn run_with_wasmtime_typed_no_args(bytes: &[u8], func_name: &str) -> i32 {
    use wasmtime::{Engine, Instance, Module, Store};
    let engine = Engine::default();
    let module = Module::new(&engine, bytes).expect("module compiles");
    let mut store: Store<()> = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).expect("instance");
    let func = instance
        .get_typed_func::<(), i32>(&mut store, func_name)
        .expect("typed func lookup");
    func.call(&mut store, ()).expect("call")
}

fn run_with_wasmtime_typed_i32_i32(bytes: &[u8], func_name: &str, a: i32, b: i32) -> i32 {
    use wasmtime::{Engine, Instance, Module, Store};
    let engine = Engine::default();
    let module = Module::new(&engine, bytes).expect("module compiles");
    let mut store: Store<()> = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).expect("instance");
    let func = instance
        .get_typed_func::<(i32, i32), i32>(&mut store, func_name)
        .expect("typed func lookup");
    func.call(&mut store, (a, b)).expect("call")
}

#[test]
fn wasmtime_runs_const_function_returns_42() {
    let f = fun("answer", vec![], vec![ret(num(42))], "i32");
    let bytes = emit_constant_function(&f).expect("emit");
    assert_eq!(run_with_wasmtime_typed_no_args(&bytes, "answer"), 42);
}

#[test]
fn wasmtime_runs_addition() {
    let body = vec![ret(binop("+", ident("a"), ident("b")))];
    let f = fun("add", vec!["a", "b"], body, "i32");
    let bytes = emit_constant_function(&f).expect("emit");
    assert_eq!(run_with_wasmtime_typed_i32_i32(&bytes, "add", 3, 4), 7);
    assert_eq!(run_with_wasmtime_typed_i32_i32(&bytes, "add", -1, 1), 0);
}

#[test]
fn wasmtime_runs_let_binding_arithmetic() {
    // fun f() -> i32 { let x = 10 ; return x + 5 }
    let body = vec![let_stmt("x", num(10)), ret(binop("+", ident("x"), num(5)))];
    let f = fun("f", vec![], body, "i32");
    let bytes = emit_constant_function(&f).expect("emit");
    assert_eq!(run_with_wasmtime_typed_no_args(&bytes, "f"), 15);
}

#[test]
fn wasmtime_runs_if_else_choosing_branches() {
    // fun choose(a, b) -> i32 { if a == b { return 1 } else { return 2 } }
    let then_body = vec![ret(num(1))];
    let else_body = vec![ret(num(2))];
    let body = vec![if_stmt(binop("==", ident("a"), ident("b")), then_body, else_body), ret(num(0))];
    let f = fun("choose", vec!["a", "b"], body, "i32");
    let bytes = emit_constant_function(&f).expect("emit");
    // Equal -> 1
    assert_eq!(run_with_wasmtime_typed_i32_i32(&bytes, "choose", 7, 7), 1);
    // Not equal -> 2
    assert_eq!(run_with_wasmtime_typed_i32_i32(&bytes, "choose", 7, 8), 2);
}

#[test]
fn wasmtime_runs_while_counter() {
    // fun count(n) -> i32 { let i = 0; while i < n { i = i + 1 }; return i }
    let body = vec![
        let_stmt("i", num(0)),
        while_stmt(
            binop("<", ident("i"), ident("n")),
            vec![assign("i", binop("+", ident("i"), num(1)))],
        ),
        ret(ident("i")),
    ];
    let f = fun("count", vec!["n"], body, "i32");
    let bytes = emit_constant_function(&f).expect("emit");
    // Count up to 5, 100, and 0 — single function with a parameter.
    use wasmtime::{Engine, Instance, Module, Store};
    let engine = Engine::default();
    let module = Module::new(&engine, &bytes).expect("module");
    let mut store: Store<()> = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).expect("instance");
    let func = instance
        .get_typed_func::<i32, i32>(&mut store, "count")
        .expect("typed func");
    assert_eq!(func.call(&mut store, 5).unwrap(), 5);
    assert_eq!(func.call(&mut store, 100).unwrap(), 100);
    assert_eq!(func.call(&mut store, 0).unwrap(), 0);
}

#[test]
fn wasmtime_runs_cross_function_call() {
    let answer = fun("answer", vec![], vec![ret(num(42))], "i32");
    let mut call = Expr::new(ExprKind::Call, 1, 1);
    call.left = Some(Box::new(ident("answer")));
    let use_answer = fun("use_answer", vec![], vec![ret(call)], "i32");
    let prog = Program {
        functions: vec![answer, use_answer],
        ..Default::default()
    };
    let bytes = emit_program(&prog).expect("emit");
    assert_eq!(run_with_wasmtime_typed_no_args(&bytes, "use_answer"), 42);
    assert_eq!(run_with_wasmtime_typed_no_args(&bytes, "answer"), 42);
}
