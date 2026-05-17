//! Full statement parser that builds Stmt nodes with real expression
//! subtrees (using `ExprParser`).
//!
//! Port of `parse_statement` and `parse_block` from
//! `native/zero-c/src/parser.c` (lines 576-716).
//!
//! Independent of the summary parser in `lib.rs`: the summary one is
//! optimized for the `parse --json` schema (which only carries decl
//! counts and bodyKinds). This one builds the full Program that the
//! Phase 4 checker walks.

use crate::expr::ExprParser;
use zero_ast::{Choice, EnumDecl, Expr, Function, Param, Program, Shape, Stmt, StmtKind};
use zero_diag::Diag;
use zero_lexer::{Token, TokenKind};

pub struct FullParser<'a> {
    pub(crate) tokens: &'a [Token],
    pub(crate) index: usize,
    pub(crate) diag: &'a mut Diag,
}

impl<'a> FullParser<'a> {
    pub fn new(tokens: &'a [Token], diag: &'a mut Diag) -> Self {
        Self { tokens, index: 0, diag }
    }

    fn current(&self) -> &Token {
        &self.tokens[self.index.min(self.tokens.len() - 1)]
    }

    fn at_eof(&self) -> bool {
        self.current().kind == TokenKind::Eof
    }

    fn check(&self, text: &str) -> bool {
        self.current().text == text
    }

    fn match_text(&mut self, text: &str) -> bool {
        if self.check(text) {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn fail(&mut self, message: &str) {
        if self.diag.code == 0 {
            let (line, column) = (self.current().line, self.current().column);
            self.diag.code = 200;
            self.diag.line = line;
            self.diag.column = column;
            self.diag.message = message.to_string();
        }
    }

    fn expect(&mut self, text: &str, message: &str) -> bool {
        if self.match_text(text) {
            true
        } else {
            self.fail(message);
            false
        }
    }

    /// Delegate to ExprParser for a single expression, advancing this
    /// parser's index by however many tokens ExprParser consumed.
    fn parse_expr(&mut self) -> Option<Expr> {
        // Slice the remaining tokens starting at our index. Build a
        // fresh ExprParser, parse one expression, then advance our
        // index by the amount it consumed.
        let mut diag = std::mem::take(self.diag);
        let mut sub = ExprParser::new(&self.tokens[self.index..], &mut diag);
        let expr = sub.parse_expr();
        let consumed = sub.position();
        self.index += consumed;
        *self.diag = diag;
        expr
    }

    fn parse_type_text(&mut self) -> String {
        // Mirror lib.rs's type parser but for our own token slice.
        let mut out = String::new();
        let mut angle = 0i32;
        let mut paren = 0i32;
        let mut bracket = 0i32;
        loop {
            if self.at_eof() {
                break;
            }
            let t = self.current().text.clone();
            match t.as_str() {
                "<" => angle += 1,
                ">" => {
                    if angle == 0 {
                        break;
                    }
                    angle -= 1;
                }
                "(" => paren += 1,
                ")" => {
                    if paren == 0 {
                        break;
                    }
                    paren -= 1;
                }
                "[" => bracket += 1,
                "]" => {
                    if bracket == 0 {
                        break;
                    }
                    bracket -= 1;
                }
                _ if angle == 0 && paren == 0 && bracket == 0 => {
                    if matches!(t.as_str(), "{" | "}" | "," | "=" | "->" | "=>" | "rescue" | "..") {
                        break;
                    }
                }
                _ => {}
            }
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(&t);
            self.index += 1;
        }
        out
    }

    pub fn parse_statement(&mut self) -> Stmt {
        let start_line = self.current().line;
        let start_col = self.current().column;

        if self.match_text("let") {
            let mut stmt = Stmt::new(StmtKind::Let, start_line, start_col);
            stmt.mutable_binding = self.match_text("mut");
            let name_tok = self.current().clone();
            if name_tok.kind != TokenKind::Ident {
                self.fail("expected binding name");
                return stmt;
            }
            self.index += 1;
            stmt.name = name_tok.text;
            if self.match_text(":") {
                stmt.type_text = self.parse_type_text();
            }
            self.expect("=", "expected '=' in let binding");
            stmt.expr = self.parse_expr().map(Box::new);
            return stmt;
        }

        // Assignment vs expression: try parsing a postfix chain that
        // ends with `=`. The simplest reliable test is to remember the
        // index, parse an expression, and see if the next token is `=`.
        if self.current().kind == TokenKind::Ident {
            let saved = self.index;
            if let Some(target) = self.parse_expr() {
                if self.check("=") && !self.check("==") {
                    self.index += 1; // consume `=`
                    let mut stmt = Stmt::new(StmtKind::Assign, start_line, start_col);
                    // Approximate: store the LHS name when it's a simple
                    // identifier; complex LHSes (a.b, arr[i]) aren't
                    // tracked in stmt.name for now.
                    if matches!(target.kind, zero_ast::ExprKind::Ident) {
                        stmt.name = target.text.clone();
                    }
                    stmt.expr = self.parse_expr().map(Box::new);
                    return stmt;
                }
                // Not an assignment — fall through as STMT_EXPR using
                // the already-parsed expression.
                let mut stmt = Stmt::new(StmtKind::Expr, start_line, start_col);
                stmt.expr = Some(Box::new(target));
                return stmt;
            } else {
                self.index = saved; // rewind on parse failure
            }
        }

        if self.match_text("defer") {
            let mut stmt = Stmt::new(StmtKind::Defer, start_line, start_col);
            stmt.expr = self.parse_expr().map(Box::new);
            return stmt;
        }
        if self.match_text("check") {
            let mut stmt = Stmt::new(StmtKind::Check, start_line, start_col);
            stmt.expr = self.parse_expr().map(Box::new);
            return stmt;
        }
        if self.match_text("return") {
            let mut stmt = Stmt::new(StmtKind::Return, start_line, start_col);
            if !self.check("}") {
                stmt.expr = self.parse_expr().map(Box::new);
            }
            return stmt;
        }
        if self.match_text("raise") {
            let mut stmt = Stmt::new(StmtKind::Raise, start_line, start_col);
            let name_tok = self.current().clone();
            if name_tok.kind == TokenKind::Ident {
                self.index += 1;
                stmt.name = name_tok.text;
            } else {
                self.fail("expected error name after raise");
            }
            return stmt;
        }
        if self.match_text("if") {
            let mut stmt = Stmt::new(StmtKind::If, start_line, start_col);
            stmt.expr = self.parse_expr().map(Box::new);
            stmt.then_body = self.parse_block();
            if self.match_text("else") {
                stmt.else_body = self.parse_block();
            }
            return stmt;
        }
        if self.match_text("while") {
            let mut stmt = Stmt::new(StmtKind::While, start_line, start_col);
            stmt.expr = self.parse_expr().map(Box::new);
            stmt.then_body = self.parse_block();
            return stmt;
        }
        if self.match_text("for") {
            let mut stmt = Stmt::new(StmtKind::For, start_line, start_col);
            let name_tok = self.current().clone();
            if name_tok.kind != TokenKind::Ident {
                self.fail("expected loop binding name");
                return stmt;
            }
            self.index += 1;
            stmt.name = name_tok.text;
            self.expect("in", "expected 'in' after loop binding");
            stmt.expr = self.parse_expr().map(Box::new);
            self.expect("..", "expected '..' in range loop");
            stmt.range_end = self.parse_expr().map(Box::new);
            stmt.then_body = self.parse_block();
            return stmt;
        }
        if self.match_text("break") {
            return Stmt::new(StmtKind::Break, start_line, start_col);
        }
        if self.match_text("continue") {
            return Stmt::new(StmtKind::Continue, start_line, start_col);
        }
        if self.match_text("match") {
            let mut stmt = Stmt::new(StmtKind::Match, start_line, start_col);
            stmt.expr = self.parse_expr().map(Box::new);
            self.expect("{", "expected '{' before match arms");
            // Skip match arms — full match-arm AST would require more
            // structure. For Phase 4 name resolution we don't need
            // arm-level detail; just skip the block balanced.
            let mut depth = 1i32;
            while !self.at_eof() && depth > 0 {
                let t = self.current().text.as_str();
                if t == "{" {
                    depth += 1;
                } else if t == "}" {
                    depth -= 1;
                    if depth == 0 {
                        self.index += 1;
                        return stmt;
                    }
                }
                self.index += 1;
            }
            return stmt;
        }

        // Default: expression statement.
        let mut stmt = Stmt::new(StmtKind::Expr, start_line, start_col);
        stmt.expr = self.parse_expr().map(Box::new);
        stmt
    }

    pub fn parse_block(&mut self) -> Vec<Stmt> {
        let mut body = Vec::new();
        if !self.expect("{", "expected '{' before block") {
            return body;
        }
        while !self.check("}") && !self.at_eof() && self.diag.code == 0 {
            body.push(self.parse_statement());
        }
        self.expect("}", "expected '}' after block");
        body
    }

    pub fn parse_param_list(&mut self) -> Vec<Param> {
        let mut params = Vec::new();
        if !self.match_text("(") {
            return params;
        }
        while !self.check(")") && !self.at_eof() && self.diag.code == 0 {
            let _ = self.match_text("static");
            let name_tok = self.current().clone();
            if name_tok.kind != TokenKind::Ident {
                self.fail("expected parameter name");
                break;
            }
            self.index += 1;
            let mut ty = String::new();
            if self.match_text(":") {
                ty = self.parse_type_text();
            }
            if self.match_text("=") {
                // skip default value via expression parse
                let _ = self.parse_expr();
            }
            params.push(Param {
                name: name_tok.text,
                ty,
                line: name_tok.line,
                column: name_tok.column,
            });
            if !self.match_text(",") {
                break;
            }
        }
        self.expect(")", "expected ')' after parameters");
        params
    }

    pub fn parse_function(&mut self, is_public: bool, decl_line: u32, decl_col: u32) -> Option<Function> {
        self.expect("fun", "expected 'fun'");
        let name_tok = self.current().clone();
        if name_tok.kind != TokenKind::Ident {
            self.fail("expected function name");
            return None;
        }
        self.index += 1;
        if self.check("<") {
            // Skip type params.
            let mut depth = 0i32;
            while !self.at_eof() {
                let t = self.current().text.as_str();
                if t == "<" {
                    depth += 1;
                } else if t == ">" {
                    depth -= 1;
                    self.index += 1;
                    if depth == 0 {
                        break;
                    }
                    continue;
                }
                self.index += 1;
            }
        }
        let params = self.parse_param_list();
        let return_type = if self.match_text("->") {
            self.parse_type_text()
        } else {
            String::new()
        };
        let raises = self.match_text("raises");
        if raises && self.check("(") {
            // Skip raises error set.
            let mut depth = 0i32;
            while !self.at_eof() {
                let t = self.current().text.as_str();
                if t == "(" {
                    depth += 1;
                } else if t == ")" {
                    depth -= 1;
                    self.index += 1;
                    if depth == 0 {
                        break;
                    }
                    continue;
                }
                self.index += 1;
            }
        }
        let body = if self.check("{") { self.parse_block() } else { Vec::new() };
        Some(Function {
            name: name_tok.text,
            return_type,
            params,
            body,
            is_public,
            raises,
            line: decl_line,
            column: decl_col,
        })
    }
}

/// Build a full Program with statement bodies populated as real ASTs.
///
/// Like `crate::parse` but uses the FullParser, which means body
/// statements carry expression trees, not just kinds.
pub fn parse_full_program(tokens: &[Token], diag: &mut Diag) -> Program {
    let mut parser = FullParser::new(tokens, diag);
    let mut program = Program::default();
    while !parser.at_eof() && parser.diag.code == 0 {
        let decl_line = parser.current().line;
        let decl_col = parser.current().column;
        let is_public = parser.match_text("pub");
        let _ = parser.match_text("extern");
        if parser.check("import") || parser.check("use") {
            // Skip import/use lines — advance until newline-ish or next
            // top-level keyword.
            while !parser.at_eof() {
                let t = parser.current().text.as_str();
                if matches!(
                    t,
                    "pub" | "fun" | "shape" | "enum" | "choice" | "const" | "type" | "interface" | "test"
                ) {
                    break;
                }
                parser.index += 1;
            }
            continue;
        }
        if parser.check("const") || parser.check("type") {
            // Skip const / alias decls.
            parser.index += 1;
            if parser.current().kind == TokenKind::Ident {
                parser.index += 1;
            }
            if parser.match_text(":") {
                let _ = parser.parse_type_text();
            }
            if parser.match_text("=") {
                let _ = parser.parse_expr();
            }
            continue;
        }
        if parser.check("test") {
            parser.index += 1;
            while !parser.check("{") && !parser.at_eof() {
                parser.index += 1;
            }
            // Skip body.
            if parser.match_text("{") {
                let mut depth = 1i32;
                while !parser.at_eof() && depth > 0 {
                    let t = parser.current().text.as_str();
                    if t == "{" {
                        depth += 1;
                    } else if t == "}" {
                        depth -= 1;
                    }
                    parser.index += 1;
                }
            }
            continue;
        }
        if parser.check("shape") {
            // For now, use the summary parser's shape logic (we can
            // re-port with full method bodies later). Skip the shape
            // block balanced and synthesize a Shape with empty fields.
            parser.index += 1; // consume "shape"
            let nm = parser.current().clone();
            if nm.kind == TokenKind::Ident {
                parser.index += 1;
            }
            // Skip type params + layout marker.
            if parser.check("<") {
                let mut depth = 0i32;
                while !parser.at_eof() {
                    let t = parser.current().text.as_str();
                    if t == "<" {
                        depth += 1;
                    } else if t == ">" {
                        depth -= 1;
                        parser.index += 1;
                        if depth == 0 {
                            break;
                        }
                        continue;
                    }
                    parser.index += 1;
                }
            }
            let _ = parser.match_text("packed");
            // Skip body.
            if parser.match_text("{") {
                let mut depth = 1i32;
                while !parser.at_eof() && depth > 0 {
                    let t = parser.current().text.as_str();
                    if t == "{" {
                        depth += 1;
                    } else if t == "}" {
                        depth -= 1;
                    }
                    parser.index += 1;
                }
            }
            program.shapes.push(Shape {
                name: nm.text,
                fields: Vec::new(),
                methods: Vec::new(),
                is_public,
                line: decl_line,
                column: decl_col,
            });
            continue;
        }
        if parser.check("enum") {
            parser.index += 1;
            let nm = parser.current().clone();
            if nm.kind == TokenKind::Ident {
                parser.index += 1;
            }
            if parser.match_text(":") {
                let _ = parser.parse_type_text();
            }
            if parser.match_text("{") {
                let mut depth = 1i32;
                while !parser.at_eof() && depth > 0 {
                    let t = parser.current().text.as_str();
                    if t == "{" {
                        depth += 1;
                    } else if t == "}" {
                        depth -= 1;
                    }
                    parser.index += 1;
                }
            }
            program.enums.push(EnumDecl {
                name: nm.text,
                cases: Vec::new(),
                line: decl_line,
                column: decl_col,
            });
            continue;
        }
        if parser.check("choice") {
            parser.index += 1;
            let nm = parser.current().clone();
            if nm.kind == TokenKind::Ident {
                parser.index += 1;
            }
            if parser.match_text("{") {
                let mut depth = 1i32;
                while !parser.at_eof() && depth > 0 {
                    let t = parser.current().text.as_str();
                    if t == "{" {
                        depth += 1;
                    } else if t == "}" {
                        depth -= 1;
                    }
                    parser.index += 1;
                }
            }
            program.choices.push(Choice {
                name: nm.text,
                cases: Vec::new(),
                line: decl_line,
                column: decl_col,
            });
            continue;
        }
        if parser.check("interface") {
            // Skip interface block.
            while !parser.check("{") && !parser.at_eof() {
                parser.index += 1;
            }
            if parser.match_text("{") {
                let mut depth = 1i32;
                while !parser.at_eof() && depth > 0 {
                    let t = parser.current().text.as_str();
                    if t == "{" {
                        depth += 1;
                    } else if t == "}" {
                        depth -= 1;
                    }
                    parser.index += 1;
                }
            }
            continue;
        }
        if parser.check("fun") {
            if let Some(f) = parser.parse_function(is_public, decl_line, decl_col) {
                program.functions.push(f);
            }
            continue;
        }
        parser.fail(&format!("unexpected top-level token '{}'", parser.current().text));
    }
    program
}

#[cfg(test)]
mod tests {
    use super::*;
    use zero_lexer::tokenize;

    fn parse(src: &str) -> Program {
        let mut diag = Diag::default();
        let tokens = tokenize(src, &mut diag);
        assert_eq!(diag.code, 0);
        let mut pdiag = Diag::default();
        let p = parse_full_program(&tokens, &mut pdiag);
        assert_eq!(pdiag.code, 0, "parse error: {:?}", pdiag);
        p
    }

    #[test]
    fn hello_world_full_ast() {
        let src = include_str!("../../../../../examples/hello.0");
        let p = parse(src);
        assert_eq!(p.functions.len(), 1);
        let f = &p.functions[0];
        assert_eq!(f.name, "main");
        assert_eq!(f.body.len(), 1);
        assert_eq!(f.body[0].kind, StmtKind::Check);
        // body[0].expr should hold the world.out.write(...) call
        let expr = f.body[0].expr.as_ref().expect("check has expr");
        assert_eq!(expr.kind, zero_ast::ExprKind::Call);
    }

    #[test]
    fn add_full_ast() {
        let src = include_str!("../../../../../examples/add.0");
        let p = parse(src);
        assert_eq!(p.functions.len(), 2);
        let main = &p.functions[1];
        assert_eq!(main.body.len(), 2);
        assert_eq!(main.body[0].kind, StmtKind::Let);
        assert_eq!(main.body[0].name, "value");
        let let_expr = main.body[0].expr.as_ref().expect("let has expr");
        assert_eq!(let_expr.kind, zero_ast::ExprKind::Call);
        assert_eq!(main.body[1].kind, StmtKind::If);
        let if_cond = main.body[1].expr.as_ref().expect("if has cond");
        assert_eq!(if_cond.kind, zero_ast::ExprKind::Binary);
        assert_eq!(if_cond.text, "==");
        // Then body has one check
        assert_eq!(main.body[1].then_body.len(), 1);
        assert_eq!(main.body[1].then_body[0].kind, StmtKind::Check);
        // Else body has one check
        assert_eq!(main.body[1].else_body.len(), 1);
    }

    #[test]
    fn branch_full_ast() {
        let src = include_str!("../../../../../examples/branch.0");
        let p = parse(src);
        let main = &p.functions[0];
        assert_eq!(main.body[0].kind, StmtKind::Let);
        assert_eq!(main.body[0].name, "ok");
        assert_eq!(main.body[1].kind, StmtKind::If);
    }

    #[test]
    fn for_loop_binding_and_range() {
        let src = r#"
fun loop_demo() -> Void {
    for i in 0 .. 10 {
        let inner = i
    }
}
"#;
        let p = parse(src);
        assert_eq!(p.functions.len(), 1);
        let body = &p.functions[0].body;
        assert_eq!(body.len(), 1);
        assert_eq!(body[0].kind, StmtKind::For);
        assert_eq!(body[0].name, "i");
        // The loop's start expr is `0` (a Number); range_end is `10`.
        let start = body[0].expr.as_ref().expect("for has start expr");
        assert_eq!(start.kind, zero_ast::ExprKind::Number);
        assert_eq!(start.text, "0");
        let end = body[0].range_end.as_ref().expect("for has range_end");
        assert_eq!(end.kind, zero_ast::ExprKind::Number);
        assert_eq!(end.text, "10");
        // Body contains one let.
        assert_eq!(body[0].then_body.len(), 1);
        assert_eq!(body[0].then_body[0].kind, StmtKind::Let);
    }

    #[test]
    fn while_with_break_and_continue() {
        let src = r#"
fun spin() -> Void {
    while true {
        break
        continue
    }
}
"#;
        let p = parse(src);
        let body = &p.functions[0].body;
        assert_eq!(body[0].kind, StmtKind::While);
        let inner = &body[0].then_body;
        assert_eq!(inner.len(), 2);
        assert_eq!(inner[0].kind, StmtKind::Break);
        assert_eq!(inner[1].kind, StmtKind::Continue);
    }

    #[test]
    fn defer_and_check_carry_expr() {
        let src = r#"
fun guarded(world: World) -> Void raises {
    defer cleanup()
    check world.out.write("hi")
}
"#;
        let p = parse(src);
        let body = &p.functions[0].body;
        assert_eq!(body[0].kind, StmtKind::Defer);
        assert_eq!(body[1].kind, StmtKind::Check);
        // Defer's expr is a call to `cleanup`
        let defer_expr = body[0].expr.as_ref().unwrap();
        assert_eq!(defer_expr.kind, zero_ast::ExprKind::Call);
        // Check's expr is a member call: world.out.write("hi")
        let check_expr = body[1].expr.as_ref().unwrap();
        assert_eq!(check_expr.kind, zero_ast::ExprKind::Call);
    }

    #[test]
    fn raise_carries_error_name() {
        let src = r#"
fun fail() -> Void raises {
    raise BadThing
}
"#;
        let p = parse(src);
        let body = &p.functions[0].body;
        assert_eq!(body[0].kind, StmtKind::Raise);
        assert_eq!(body[0].name, "BadThing");
    }

    #[test]
    fn nested_if_inside_while() {
        let src = r#"
fun nested(n: i32) -> i32 {
    let i = 0
    while i < n {
        if i == 3 {
            return i
        }
        i = i + 1
    }
    return 0
}
"#;
        let p = parse(src);
        let body = &p.functions[0].body;
        assert_eq!(body[1].kind, StmtKind::While);
        let loop_body = &body[1].then_body;
        assert_eq!(loop_body[0].kind, StmtKind::If);
        // Then-body of the inner if: one return
        assert_eq!(loop_body[0].then_body.len(), 1);
        assert_eq!(loop_body[0].then_body[0].kind, StmtKind::Return);
        // Assignment after the if
        assert_eq!(loop_body[1].kind, StmtKind::Assign);
        assert_eq!(loop_body[1].name, "i");
    }

    #[test]
    fn let_with_type_annotation() {
        let src = r#"
fun typed() -> Void {
    let mut buf: i32 = 0
}
"#;
        let p = parse(src);
        let body = &p.functions[0].body;
        assert_eq!(body[0].kind, StmtKind::Let);
        assert_eq!(body[0].name, "buf");
        assert_eq!(body[0].type_text, "i32");
        assert!(body[0].mutable_binding);
    }

    #[test]
    fn return_without_expr_in_void() {
        let src = r#"
fun nothing() -> Void {
    return
}
"#;
        let p = parse(src);
        let body = &p.functions[0].body;
        assert_eq!(body[0].kind, StmtKind::Return);
        assert!(body[0].expr.is_none());
    }

    #[test]
    fn multiple_top_level_decls() {
        let src = r#"
shape Point { x: i32, y: i32 }

enum Color { Red, Green, Blue }

fun area(p: Point) -> i32 {
    return p.x * p.y
}

pub fun main() -> Void {
}
"#;
        let p = parse(src);
        assert_eq!(p.shapes.len(), 1);
        assert_eq!(p.shapes[0].name, "Point");
        assert_eq!(p.enums.len(), 1);
        assert_eq!(p.enums[0].name, "Color");
        assert_eq!(p.functions.len(), 2);
        // Public visibility flag is preserved
        assert!(!p.functions[0].is_public);
        assert!(p.functions[1].is_public);
    }
}
