//! Phase 4 (partial): name resolution checker.
//!
//! Walks the full Program AST (from `zero_parser::parse_full_program`)
//! and reports references to identifiers that aren't in scope. This is
//! a small but real subset of what the C checker does — it catches
//! typos and forgotten declarations. The full Phase 4 work (type
//! checking, borrow checking, effect checking, generic instantiation,
//! interface dispatch, match exhaustiveness) is much larger and lives
//! in subsequent PRs.

use indexmap::IndexSet;
use zero_ast::{Expr, ExprKind, Program, Stmt, StmtKind};
use zero_diag::Diag;

/// Built-in identifiers that are always in scope.
const BUILTINS: &[&str] = &[
    // Standard library prefix (anything starting with `std` is allowed).
    "std",
    // Capability handle threaded through main(world: World).
    "world",
    // Boolean / null literals lex as keywords but may appear as ident
    // references in some contexts.
    "true", "false", "null",
];

fn is_builtin(name: &str) -> bool {
    BUILTINS.contains(&name)
}

#[derive(Default)]
struct Scope {
    // Stack of scope frames — each frame is the set of names introduced
    // at that nesting level.
    frames: Vec<IndexSet<String>>,
    // Set of all top-level decl names (functions, shapes, enums, choices)
    // that are always visible from anywhere in the program.
    module: IndexSet<String>,
}

impl Scope {
    fn enter(&mut self) {
        self.frames.push(IndexSet::new());
    }
    fn exit(&mut self) {
        self.frames.pop();
    }
    fn define(&mut self, name: &str) {
        if let Some(top) = self.frames.last_mut() {
            top.insert(name.to_string());
        }
    }
    fn is_defined(&self, name: &str) -> bool {
        if is_builtin(name) {
            return true;
        }
        if name.starts_with("std") {
            return true;
        }
        if self.module.contains(name) {
            return true;
        }
        for frame in self.frames.iter().rev() {
            if frame.contains(name) {
                return true;
            }
        }
        false
    }
}

/// Walk the program and return all name-resolution diagnostics found.
///
/// Each diagnostic has `code = 3002` (NAM002 — "undefined name") and
/// the line/column of the offending identifier.
pub fn resolve_names(program: &Program) -> Vec<Diag> {
    let mut scope = Scope::default();
    // Collect all top-level names first.
    for f in &program.functions {
        scope.module.insert(f.name.clone());
    }
    for s in &program.shapes {
        scope.module.insert(s.name.clone());
    }
    for e in &program.enums {
        scope.module.insert(e.name.clone());
    }
    for c in &program.choices {
        scope.module.insert(c.name.clone());
    }

    let mut diags = Vec::new();
    for f in &program.functions {
        scope.enter();
        for param in &f.params {
            scope.define(&param.name);
        }
        for stmt in &f.body {
            walk_stmt(stmt, &mut scope, &mut diags);
        }
        scope.exit();
    }
    diags
}

fn walk_stmt(stmt: &Stmt, scope: &mut Scope, diags: &mut Vec<Diag>) {
    match stmt.kind {
        StmtKind::Let => {
            if let Some(e) = &stmt.expr {
                walk_expr(e, scope, diags);
            }
            scope.define(&stmt.name);
        }
        StmtKind::Assign => {
            // LHS name should already be defined (it's an assign, not a
            // declaration). The simple-ident case is recorded in name;
            // more complex LHSes (a.b.c, arr[i]) aren't tracked here.
            if !stmt.name.is_empty() && !scope.is_defined(&stmt.name) {
                diags.push(undefined_diag(&stmt.name, stmt.line, stmt.column));
            }
            if let Some(e) = &stmt.expr {
                walk_expr(e, scope, diags);
            }
        }
        StmtKind::For => {
            // The loop binding is defined for the body.
            if let Some(e) = &stmt.expr {
                walk_expr(e, scope, diags);
            }
            if let Some(e) = &stmt.range_end {
                walk_expr(e, scope, diags);
            }
            scope.enter();
            scope.define(&stmt.name);
            for inner in &stmt.then_body {
                walk_stmt(inner, scope, diags);
            }
            scope.exit();
        }
        StmtKind::If | StmtKind::While => {
            if let Some(e) = &stmt.expr {
                walk_expr(e, scope, diags);
            }
            scope.enter();
            for inner in &stmt.then_body {
                walk_stmt(inner, scope, diags);
            }
            scope.exit();
            if !stmt.else_body.is_empty() {
                scope.enter();
                for inner in &stmt.else_body {
                    walk_stmt(inner, scope, diags);
                }
                scope.exit();
            }
        }
        _ => {
            if let Some(e) = &stmt.expr {
                walk_expr(e, scope, diags);
            }
        }
    }
}

fn walk_expr(expr: &Expr, scope: &mut Scope, diags: &mut Vec<Diag>) {
    match expr.kind {
        ExprKind::Ident if !scope.is_defined(&expr.text) => {
            diags.push(undefined_diag(&expr.text, expr.line, expr.column));
        }
        ExprKind::Ident => {}
        ExprKind::Member => {
            // Only the base (left) of a member chain is a reference;
            // the member name itself is a field lookup.
            if let Some(left) = &expr.left {
                walk_expr(left, scope, diags);
            }
        }
        ExprKind::ShapeLiteral => {
            // The shape name should be a known type at module scope.
            if !expr.text.is_empty() && !scope.is_defined(&expr.text) {
                diags.push(undefined_diag(&expr.text, expr.line, expr.column));
            }
            for field in &expr.fields {
                walk_expr(&field.value, scope, diags);
            }
        }
        ExprKind::Call
        | ExprKind::Binary
        | ExprKind::Index
        | ExprKind::Slice
        | ExprKind::Cast
        | ExprKind::Borrow
        | ExprKind::Check
        | ExprKind::Rescue
        | ExprKind::Meta
        | ExprKind::ArrayLiteral => {
            if let Some(left) = &expr.left {
                walk_expr(left, scope, diags);
            }
            if let Some(right) = &expr.right {
                walk_expr(right, scope, diags);
            }
            for arg in &expr.args {
                walk_expr(arg, scope, diags);
            }
        }
        _ => {}
    }
}

fn undefined_diag(name: &str, line: u32, column: u32) -> Diag {
    let mut d = Diag {
        code: 3002,
        line,
        column,
        length: name.len() as u32,
        ..Default::default()
    };
    d.message = format!("undefined name '{name}'");
    d.expected = "name defined in scope, a parameter, or a top-level decl".to_string();
    d.actual = format!("'{name}' is not in scope");
    d.help = "check spelling, add a let binding, or import the symbol".to_string();
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use zero_diag::Diag;
    use zero_lexer::tokenize;
    use zero_parser::parse_full_program;

    fn check_program(src: &str) -> Vec<Diag> {
        let mut ldiag = Diag::default();
        let tokens = tokenize(src, &mut ldiag);
        assert_eq!(ldiag.code, 0, "lex error");
        let mut pdiag = Diag::default();
        let program = parse_full_program(&tokens, &mut pdiag);
        assert_eq!(pdiag.code, 0, "parse error: {:?}", pdiag);
        resolve_names(&program)
    }

    #[test]
    fn hello_world_has_no_undefined_names() {
        let src = include_str!("../../../../../examples/hello.0");
        let diags = check_program(src);
        assert!(
            diags.is_empty(),
            "hello.0 should have no undefined names, got: {diags:?}"
        );
    }

    #[test]
    fn add_example_has_no_undefined_names() {
        let src = include_str!("../../../../../examples/add.0");
        let diags = check_program(src);
        assert!(diags.is_empty(), "add.0: {diags:?}");
    }

    #[test]
    fn detects_undefined_variable_in_let_rhs() {
        let src = r#"
fun main() -> Void {
    let x = does_not_exist
}
"#;
        let diags = check_program(src);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, 3002);
        assert!(diags[0].message.contains("does_not_exist"));
    }

    #[test]
    fn detects_undefined_variable_in_expr() {
        let src = r#"
fun main() -> Void {
    let x = 1
    let y = z + x
}
"#;
        let diags = check_program(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("'z'"));
    }

    #[test]
    fn accepts_let_then_reference() {
        let src = r#"
fun main() -> Void {
    let x = 1
    let y = x
}
"#;
        let diags = check_program(src);
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn accepts_param_reference() {
        let src = r#"
fun greet(name: String) -> String {
    name
}
"#;
        let diags = check_program(src);
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn detects_undefined_at_call_site() {
        let src = r#"
fun main() -> Void {
    unknown_fn(1, 2)
}
"#;
        let diags = check_program(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("unknown_fn"));
    }

    #[test]
    fn accepts_top_level_function_reference() {
        let src = r#"
fun answer() -> i32 {
    return 42
}

fun main() -> Void {
    let v = answer()
}
"#;
        let diags = check_program(src);
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn std_prefix_always_in_scope() {
        let src = r#"
fun main() -> Void {
    let v = std.mem.copy(a, b)
}
"#;
        // a, b are undefined — the rest (std.mem.copy chain) is OK.
        let diags = check_program(src);
        assert_eq!(diags.len(), 2);
        let names: Vec<&str> = diags.iter().map(|d| d.message.as_str()).collect();
        assert!(names.iter().any(|m| m.contains("'a'")));
        assert!(names.iter().any(|m| m.contains("'b'")));
    }

    #[test]
    fn world_param_is_in_scope_after_declaration() {
        // World as a param name should be in scope inside main().
        let src = r#"
fun main(world: World) -> Void raises {
    check world.out.write("hi")
}
"#;
        let diags = check_program(src);
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn if_scope_does_not_leak() {
        // Variables declared inside an `if` body must not be visible
        // outside the block.
        let src = r#"
fun main() -> Void {
    if true {
        let inner = 1
    }
    let x = inner
}
"#;
        let diags = check_program(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("'inner'"));
    }

    #[test]
    fn while_scope_does_not_leak() {
        let src = r#"
fun main() -> Void {
    while true {
        let counter = 0
    }
    let x = counter
}
"#;
        let diags = check_program(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("'counter'"));
    }

    #[test]
    fn else_branch_has_its_own_scope() {
        let src = r#"
fun main() -> Void {
    if true {
        let a = 1
    } else {
        let a = 2
    }
    let x = a
}
"#;
        let diags = check_program(src);
        // `a` defined in both branches but never escapes either; outer
        // reference is undefined.
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("'a'"));
    }

    #[test]
    fn for_loop_binding_visible_only_inside_body() {
        let src = r#"
fun main() -> Void {
    for i in 0 .. 5 {
        let inner = i
    }
    let x = i
}
"#;
        let diags = check_program(src);
        // `i` (the loop binding) AND `inner` are both out of scope after
        // the loop.
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("'i'"));
    }

    #[test]
    fn shape_name_is_in_scope() {
        let src = r#"
shape Point { x: i32, y: i32 }

fun origin() -> Point {
    return Point { x: 0, y: 0 }
}
"#;
        let diags = check_program(src);
        assert!(diags.is_empty(), "shape ref should resolve: {diags:?}");
    }

    #[test]
    fn shape_literal_with_unknown_type_errors() {
        let src = r#"
fun bad() -> Void {
    let p = Nonexistent { x: 1 }
}
"#;
        let diags = check_program(src);
        assert!(!diags.is_empty(), "Nonexistent shape should produce diag");
        assert!(diags[0].message.contains("'Nonexistent'"));
    }

    #[test]
    fn enum_name_is_in_scope() {
        let src = r#"
enum Color { Red, Green, Blue }

fun pick() -> Void {
    let c = Color
}
"#;
        let diags = check_program(src);
        assert!(diags.is_empty(), "enum ref should resolve: {diags:?}");
    }

    #[test]
    fn multiple_undefined_names_all_reported() {
        let src = r#"
fun main() -> Void {
    let a = unknown_one
    let b = unknown_two
    let c = unknown_three
}
"#;
        let diags = check_program(src);
        assert_eq!(diags.len(), 3);
        let messages: Vec<&str> = diags.iter().map(|d| d.message.as_str()).collect();
        assert!(messages.iter().any(|m| m.contains("unknown_one")));
        assert!(messages.iter().any(|m| m.contains("unknown_two")));
        assert!(messages.iter().any(|m| m.contains("unknown_three")));
    }

    #[test]
    fn assignment_to_undefined_name_errors() {
        let src = r#"
fun main() -> Void {
    nonexistent = 1
}
"#;
        let diags = check_program(src);
        assert!(!diags.is_empty());
        // First diagnostic should mention the undefined LHS.
        assert!(diags[0].message.contains("nonexistent"));
    }

    #[test]
    fn shadowing_is_allowed() {
        // Inner block re-declaring an outer name is fine — Zero scope
        // rules allow shadowing inside nested blocks.
        let src = r#"
fun main() -> Void {
    let x = 1
    if true {
        let x = 2
        let y = x
    }
    let z = x
}
"#;
        let diags = check_program(src);
        assert!(diags.is_empty(), "shadowing should not error: {diags:?}");
    }

    #[test]
    fn diagnostics_carry_correct_line_column() {
        let src = "fun main() -> Void {\n    let x = unknown\n}\n";
        let diags = check_program(src);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 2);
        // `unknown` starts at column 13 (1-indexed, after "    let x = ").
        assert_eq!(diags[0].column, 13);
        assert_eq!(diags[0].length, "unknown".len() as u32);
    }
}
