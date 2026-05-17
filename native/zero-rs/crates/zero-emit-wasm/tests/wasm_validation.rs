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

#[test]
fn wasmtime_arithmetic_subtraction() {
    let body = vec![ret(binop("-", ident("a"), ident("b")))];
    let f = fun("sub", vec!["a", "b"], body, "i32");
    let bytes = emit_constant_function(&f).expect("emit");
    assert_eq!(run_with_wasmtime_typed_i32_i32(&bytes, "sub", 10, 3), 7);
    assert_eq!(run_with_wasmtime_typed_i32_i32(&bytes, "sub", 0, 5), -5);
    assert_eq!(run_with_wasmtime_typed_i32_i32(&bytes, "sub", 42, 42), 0);
}

#[test]
fn wasmtime_arithmetic_multiplication() {
    let body = vec![ret(binop("*", ident("a"), ident("b")))];
    let f = fun("mul", vec!["a", "b"], body, "i32");
    let bytes = emit_constant_function(&f).expect("emit");
    assert_eq!(run_with_wasmtime_typed_i32_i32(&bytes, "mul", 7, 6), 42);
    assert_eq!(run_with_wasmtime_typed_i32_i32(&bytes, "mul", 0, 1000), 0);
    assert_eq!(run_with_wasmtime_typed_i32_i32(&bytes, "mul", -3, 4), -12);
}

#[test]
fn wasmtime_arithmetic_division_signed() {
    let body = vec![ret(binop("/", ident("a"), ident("b")))];
    let f = fun("div", vec!["a", "b"], body, "i32");
    let bytes = emit_constant_function(&f).expect("emit");
    assert_eq!(run_with_wasmtime_typed_i32_i32(&bytes, "div", 10, 2), 5);
    assert_eq!(run_with_wasmtime_typed_i32_i32(&bytes, "div", 9, 2), 4);     // signed trunc toward zero
    assert_eq!(run_with_wasmtime_typed_i32_i32(&bytes, "div", -9, 2), -4);   // signed trunc toward zero
}

#[test]
fn wasmtime_each_comparison_returns_0_or_1() {
    // Build a function that returns 1 if a == b, else 0. WASM
    // comparison ops push i32 0/1 onto the stack; verify the result
    // is exactly 0 or 1.
    for (op, expected_yes_inputs) in [
        ("==", (5, 5)),
        ("!=", (5, 6)),
        ("<", (3, 7)),
        ("<=", (5, 5)),
        (">", (9, 2)),
        (">=", (5, 5)),
    ] {
        let body = vec![ret(binop(op, ident("a"), ident("b")))];
        let f = fun("cmp", vec!["a", "b"], body, "i32");
        let bytes = emit_constant_function(&f).expect("emit");
        let (a, b) = expected_yes_inputs;
        assert_eq!(
            run_with_wasmtime_typed_i32_i32(&bytes, "cmp", a, b),
            1,
            "op {op} should be true for ({a}, {b})"
        );
    }
}

#[test]
fn wasmtime_runs_deeply_nested_arithmetic() {
    // ((a + b) * 2) - (c - 1)
    let inner_sum = binop("+", ident("a"), ident("b"));
    let times_two = binop("*", inner_sum, num(2));
    let inner_sub = binop("-", ident("c"), num(1));
    let outer = binop("-", times_two, inner_sub);
    let body = vec![ret(outer)];
    let mut f = fun("compute", vec!["a", "b"], body, "i32");
    f.params.push(zero_ast::Param {
        name: "c".into(),
        ty: "i32".into(),
        line: 1,
        column: 1,
    });
    let bytes = emit_constant_function(&f).expect("emit");
    // For (a=3, b=4, c=10): ((3+4)*2) - (10-1) = 14 - 9 = 5
    use wasmtime::{Engine, Instance, Module, Store};
    let engine = Engine::default();
    let module = Module::new(&engine, &bytes).expect("module");
    let mut store: Store<()> = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).expect("instance");
    let func = instance
        .get_typed_func::<(i32, i32, i32), i32>(&mut store, "compute")
        .expect("typed func");
    assert_eq!(func.call(&mut store, (3, 4, 10)).unwrap(), 5);
    assert_eq!(func.call(&mut store, (1, 1, 5)).unwrap(), 0);
    assert_eq!(func.call(&mut store, (0, 0, 0)).unwrap(), 1);
}

#[test]
fn wasmtime_chained_let_bindings() {
    // fun chain() -> i32 { let a = 10; let b = a + 5; let c = b * 2; return c }
    let body = vec![
        let_stmt("a", num(10)),
        let_stmt("b", binop("+", ident("a"), num(5))),
        let_stmt("c", binop("*", ident("b"), num(2))),
        ret(ident("c")),
    ];
    let f = fun("chain", vec![], body, "i32");
    let bytes = emit_constant_function(&f).expect("emit");
    assert_eq!(run_with_wasmtime_typed_no_args(&bytes, "chain"), 30);
}

#[test]
fn wasmtime_if_without_else_falls_through() {
    // fun maybe(n: i32) -> i32 { let r = 100; if n == 0 { return 7 } ; return r }
    let then_body = vec![ret(num(7))];
    let body = vec![
        let_stmt("r", num(100)),
        if_stmt(binop("==", ident("n"), num(0)), then_body, vec![]),
        ret(ident("r")),
    ];
    let f = fun("maybe", vec!["n"], body, "i32");
    let bytes = emit_constant_function(&f).expect("emit");
    use wasmtime::{Engine, Instance, Module, Store};
    let engine = Engine::default();
    let module = Module::new(&engine, &bytes).expect("module");
    let mut store: Store<()> = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).expect("instance");
    let func = instance
        .get_typed_func::<i32, i32>(&mut store, "maybe")
        .expect("typed func");
    assert_eq!(func.call(&mut store, 0).unwrap(), 7);
    assert_eq!(func.call(&mut store, 5).unwrap(), 100);
}

#[test]
fn wasmtime_while_sum_to_n() {
    // fun sum(n: i32) -> i32 { let s = 0; let i = 1; while i <= n { s = s + i; i = i + 1 } ; return s }
    let body = vec![
        let_stmt("s", num(0)),
        let_stmt("i", num(1)),
        while_stmt(
            binop("<=", ident("i"), ident("n")),
            vec![
                assign("s", binop("+", ident("s"), ident("i"))),
                assign("i", binop("+", ident("i"), num(1))),
            ],
        ),
        ret(ident("s")),
    ];
    let f = fun("sum", vec!["n"], body, "i32");
    let bytes = emit_constant_function(&f).expect("emit");
    use wasmtime::{Engine, Instance, Module, Store};
    let engine = Engine::default();
    let module = Module::new(&engine, &bytes).expect("module");
    let mut store: Store<()> = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).expect("instance");
    let func = instance
        .get_typed_func::<i32, i32>(&mut store, "sum")
        .expect("typed func");
    // 1+2+...+10 = 55
    assert_eq!(func.call(&mut store, 10).unwrap(), 55);
    // 1+2+...+100 = 5050
    assert_eq!(func.call(&mut store, 100).unwrap(), 5050);
    // sum(0) = 0 (the while never enters)
    assert_eq!(func.call(&mut store, 0).unwrap(), 0);
}

#[test]
fn wasmtime_call_with_argument() {
    // fun double(x: i32) -> i32 { return x + x }
    // fun use_double(n: i32) -> i32 { return double(n) }
    let double_body = vec![ret(binop("+", ident("x"), ident("x")))];
    let double = fun("double", vec!["x"], double_body, "i32");
    let mut call = Expr::new(ExprKind::Call, 1, 1);
    call.left = Some(Box::new(ident("double")));
    call.args.push(ident("n"));
    let use_body = vec![ret(call)];
    let use_double = fun("use_double", vec!["n"], use_body, "i32");
    let prog = Program {
        functions: vec![double, use_double],
        ..Default::default()
    };
    let bytes = emit_program(&prog).expect("emit");
    use wasmtime::{Engine, Instance, Module, Store};
    let engine = Engine::default();
    let module = Module::new(&engine, &bytes).expect("module");
    let mut store: Store<()> = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).expect("instance");
    let func = instance
        .get_typed_func::<i32, i32>(&mut store, "use_double")
        .expect("typed func");
    assert_eq!(func.call(&mut store, 21).unwrap(), 42);
    assert_eq!(func.call(&mut store, 0).unwrap(), 0);
    assert_eq!(func.call(&mut store, -7).unwrap(), -14);
}

#[test]
fn wasmtime_continue_skips_iteration() {
    // fun even_sum(n: i32) -> i32 {
    //   let s = 0; let i = 0;
    //   while i < n {
    //     i = i + 1
    //     if i == 3 { continue }   // skip i=3
    //     s = s + i
    //   }
    //   return s
    // }
    let inc_i = assign("i", binop("+", ident("i"), num(1)));
    let mut continue_stmt = zero_ast::Stmt::new(StmtKind::Continue, 1, 1);
    let _ = &mut continue_stmt;
    let skip_three = if_stmt(
        binop("==", ident("i"), num(3)),
        vec![zero_ast::Stmt::new(StmtKind::Continue, 1, 1)],
        vec![],
    );
    let add_to_s = assign("s", binop("+", ident("s"), ident("i")));
    let body = vec![
        let_stmt("s", num(0)),
        let_stmt("i", num(0)),
        while_stmt(
            binop("<", ident("i"), ident("n")),
            vec![inc_i, skip_three, add_to_s],
        ),
        ret(ident("s")),
    ];
    let f = fun("even_sum", vec!["n"], body, "i32");
    let bytes = emit_constant_function(&f).expect("emit");
    use wasmtime::{Engine, Instance, Module, Store};
    let engine = Engine::default();
    let module = Module::new(&engine, &bytes).expect("module");
    let mut store: Store<()> = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).expect("instance");
    let func = instance
        .get_typed_func::<i32, i32>(&mut store, "even_sum")
        .expect("typed func");
    // n=5: 1+2+4+5 = 12 (skip 3)
    assert_eq!(func.call(&mut store, 5).unwrap(), 12);
    // n=10: 1+2+4+5+6+7+8+9+10 = 52 (skip 3)
    assert_eq!(func.call(&mut store, 10).unwrap(), 52);
}

#[test]
fn wasmtime_break_exits_while_immediately() {
    // fun first_match(target: i32) -> i32 {
    //   let i = 0
    //   while i < 100 {
    //     i = i + 1
    //     if i == target { break }
    //   }
    //   return i
    // }
    let body = vec![
        let_stmt("i", num(0)),
        while_stmt(
            binop("<", ident("i"), num(100)),
            vec![
                assign("i", binop("+", ident("i"), num(1))),
                if_stmt(
                    binop("==", ident("i"), ident("target")),
                    vec![zero_ast::Stmt::new(StmtKind::Break, 1, 1)],
                    vec![],
                ),
            ],
        ),
        ret(ident("i")),
    ];
    let f = fun("first_match", vec!["target"], body, "i32");
    let bytes = emit_constant_function(&f).expect("emit");
    use wasmtime::{Engine, Instance, Module, Store};
    let engine = Engine::default();
    let module = Module::new(&engine, &bytes).expect("module");
    let mut store: Store<()> = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).expect("instance");
    let func = instance
        .get_typed_func::<i32, i32>(&mut store, "first_match")
        .expect("typed func");
    assert_eq!(func.call(&mut store, 7).unwrap(), 7);
    assert_eq!(func.call(&mut store, 1).unwrap(), 1);
    // target above loop bound: loop runs to completion
    assert_eq!(func.call(&mut store, 200).unwrap(), 100);
}

#[test]
fn wasmtime_recursion_factorial() {
    // fun fact(n: i32) -> i32 {
    //   if n <= 1 { return 1 }
    //   return n * fact(n - 1)
    // }
    let mut recur_call = Expr::new(ExprKind::Call, 1, 1);
    recur_call.left = Some(Box::new(ident("fact")));
    recur_call.args.push(binop("-", ident("n"), num(1)));
    let body = vec![
        if_stmt(
            binop("<=", ident("n"), num(1)),
            vec![ret(num(1))],
            vec![],
        ),
        ret(binop("*", ident("n"), recur_call)),
    ];
    let f = fun("fact", vec!["n"], body, "i32");
    let prog = Program {
        functions: vec![f],
        ..Default::default()
    };
    let bytes = emit_program(&prog).expect("emit");
    use wasmtime::{Engine, Instance, Module, Store};
    let engine = Engine::default();
    let module = Module::new(&engine, &bytes).expect("module");
    let mut store: Store<()> = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).expect("instance");
    let func = instance
        .get_typed_func::<i32, i32>(&mut store, "fact")
        .expect("typed func");
    assert_eq!(func.call(&mut store, 0).unwrap(), 1);
    assert_eq!(func.call(&mut store, 1).unwrap(), 1);
    assert_eq!(func.call(&mut store, 5).unwrap(), 120);
    assert_eq!(func.call(&mut store, 10).unwrap(), 3_628_800);
}
