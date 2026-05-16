//! Expression-tree parser (Phase 3.5).
//!
//! Direct port of the expression-parsing chunk of
//! `native/zero-c/src/parser.c` (lines 349-566): parse_primary,
//! parse_postfix, parse_unary, parse_binary with precedence climbing,
//! and parse_expr with `as`/`rescue` handling.
//!
//! Used by callers that need real expression ASTs (Phase 4 checker
//! and later). The summary parser in `lib.rs` continues to use the
//! token-skip approach for the `parse --json` schema, which only
//! exposes high-level decl counts.

use zero_ast::{Expr, ExprKind, FieldInit};
use zero_diag::Diag;
use zero_lexer::{Token, TokenKind};

/// Precedence table mirroring `precedence()` in parser.c:349.
fn precedence(op: &str) -> i32 {
    match op {
        "||" => 1,
        "&&" => 2,
        "==" | "!=" | "<" | "<=" | ">" | ">=" => 3,
        "+" | "-" | "+%" | "+|" => 4,
        "*" | "/" | "%" => 5,
        _ => -1,
    }
}

pub struct ExprParser<'a> {
    tokens: &'a [Token],
    index: usize,
    diag: &'a mut Diag,
}

impl<'a> ExprParser<'a> {
    pub fn new(tokens: &'a [Token], diag: &'a mut Diag) -> Self {
        Self { tokens, index: 0, diag }
    }

    pub fn position(&self) -> usize {
        self.index
    }

    pub fn current(&self) -> &Token {
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

    pub fn parse_expr(&mut self) -> Option<Expr> {
        let mut expr = self.parse_binary(0)?;
        // `expr as TYPE` chain.
        while self.match_text("as") {
            let mut cast = Expr::new(ExprKind::Cast, expr.line, expr.column);
            cast.text = self.parse_type_text();
            cast.left = Some(Box::new(expr));
            expr = cast;
        }
        // `expr rescue NAME { fallback }`
        if self.match_text("rescue") {
            let mut rescue = Expr::new(ExprKind::Rescue, expr.line, expr.column);
            let name_tok = self.current().clone();
            if name_tok.kind == TokenKind::Ident {
                self.index += 1;
                rescue.text = name_tok.text;
            } else {
                self.fail("expected error binding after rescue");
            }
            self.expect("{", "expected '{' before rescue fallback");
            if let Some(fallback) = self.parse_expr() {
                rescue.right = Some(Box::new(fallback));
            }
            self.expect("}", "expected '}' after rescue fallback");
            rescue.left = Some(Box::new(expr));
            expr = rescue;
        }
        Some(expr)
    }

    fn parse_binary(&mut self, min_precedence: i32) -> Option<Expr> {
        let mut left = self.parse_unary()?;
        loop {
            let prec = precedence(&self.current().text);
            if prec < min_precedence {
                return Some(left);
            }
            let op_tok = self.current().clone();
            self.index += 1;
            let right = self.parse_binary(prec + 1);
            let mut bin = Expr::new(ExprKind::Binary, op_tok.line, op_tok.column);
            bin.text = op_tok.text;
            bin.left = Some(Box::new(left));
            if let Some(r) = right {
                bin.right = Some(Box::new(r));
            }
            left = bin;
        }
    }

    fn parse_unary(&mut self) -> Option<Expr> {
        let tok = self.current().clone();
        if self.match_text("meta") {
            let mut e = Expr::new(ExprKind::Meta, tok.line, tok.column);
            e.left = self.parse_unary().map(Box::new);
            return Some(e);
        }
        if self.match_text("check") {
            let mut e = Expr::new(ExprKind::Check, tok.line, tok.column);
            e.left = self.parse_unary().map(Box::new);
            return Some(e);
        }
        if self.match_text("&") {
            let mutable_borrow = self.match_text("mut");
            let mut e = Expr::new(ExprKind::Borrow, tok.line, tok.column);
            e.mutable_borrow = mutable_borrow;
            e.left = self.parse_unary().map(Box::new);
            return Some(e);
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Option<Expr> {
        let mut expr = self.parse_primary()?;
        loop {
            if self.match_text(".") {
                let property = self.current().clone();
                if property.kind != TokenKind::Ident {
                    self.fail("expected field name after '.'");
                    return Some(expr);
                }
                self.index += 1;
                let mut member = Expr::new(ExprKind::Member, property.line, property.column);
                member.text = property.text;
                member.left = Some(Box::new(expr));
                expr = member;
                continue;
            }
            if self.match_text("[") {
                let index_tok = (expr.line, expr.column);
                // Slice with no start: `[..end]`
                if self.match_text("..") {
                    let mut slice = Expr::new(ExprKind::Slice, index_tok.0, index_tok.1);
                    slice.left = Some(Box::new(expr));
                    // empty start sentinel: push a marker arg? Use line=0/col=0 placeholder.
                    slice.args.push(empty_marker());
                    let end = if self.check("]") {
                        empty_marker()
                    } else {
                        self.parse_expr().unwrap_or_else(empty_marker)
                    };
                    slice.args.push(end);
                    self.expect("]", "expected ']' after slice expression");
                    expr = slice;
                    continue;
                }
                let start = self.parse_expr().unwrap_or_else(empty_marker);
                if self.match_text("..") {
                    let mut slice = Expr::new(ExprKind::Slice, index_tok.0, index_tok.1);
                    slice.left = Some(Box::new(expr));
                    slice.args.push(start);
                    let end = if self.check("]") {
                        empty_marker()
                    } else {
                        self.parse_expr().unwrap_or_else(empty_marker)
                    };
                    slice.args.push(end);
                    self.expect("]", "expected ']' after slice expression");
                    expr = slice;
                    continue;
                }
                let mut index = Expr::new(ExprKind::Index, index_tok.0, index_tok.1);
                index.left = Some(Box::new(expr));
                index.right = Some(Box::new(start));
                self.expect("]", "expected ']' after index expression");
                expr = index;
                continue;
            }
            if self.match_text("(") {
                let mut call = Expr::new(ExprKind::Call, expr.line, expr.column);
                call.left = Some(Box::new(expr));
                if !self.match_text(")") {
                    loop {
                        if let Some(arg) = self.parse_expr() {
                            call.args.push(arg);
                        }
                        if !self.match_text(",") {
                            break;
                        }
                    }
                    self.expect(")", "expected ')' after arguments");
                }
                expr = call;
                continue;
            }
            return Some(expr);
        }
    }

    fn parse_primary(&mut self) -> Option<Expr> {
        let tok = self.current().clone();
        match tok.kind {
            TokenKind::Ident => {
                self.index += 1;
                let first_char = tok.text.chars().next();
                let is_uppercase_start = first_char.is_some_and(|c| c.is_ascii_uppercase());
                if is_uppercase_start && self.check("{") {
                    self.index += 1;
                    let mut expr = Expr::new(ExprKind::ShapeLiteral, tok.line, tok.column);
                    expr.text = tok.text;
                    if !self.match_text("}") {
                        loop {
                            let field_tok = self.current().clone();
                            if field_tok.kind != TokenKind::Ident {
                                self.fail("expected shape literal field name");
                                break;
                            }
                            self.index += 1;
                            self.expect(":", "expected ':' after shape literal field name");
                            let value = self.parse_expr()?;
                            expr.fields.push(FieldInit {
                                name: field_tok.text,
                                value: Box::new(value),
                                line: field_tok.line,
                                column: field_tok.column,
                            });
                            if !self.match_text(",") {
                                break;
                            }
                        }
                        self.expect("}", "expected '}' after shape literal");
                    }
                    return Some(expr);
                }
                let mut expr = Expr::new(ExprKind::Ident, tok.line, tok.column);
                expr.text = tok.text;
                Some(expr)
            }
            TokenKind::String => {
                self.index += 1;
                let mut expr = Expr::new(ExprKind::String, tok.line, tok.column);
                expr.text = tok.text;
                Some(expr)
            }
            TokenKind::Char => {
                self.index += 1;
                let mut expr = Expr::new(ExprKind::Char, tok.line, tok.column);
                expr.text = tok.text;
                Some(expr)
            }
            TokenKind::Number => {
                self.index += 1;
                let mut expr = Expr::new(ExprKind::Number, tok.line, tok.column);
                expr.text = tok.text;
                Some(expr)
            }
            TokenKind::Keyword => match tok.text.as_str() {
                "true" | "false" => {
                    self.index += 1;
                    let mut expr = Expr::new(ExprKind::Bool, tok.line, tok.column);
                    expr.bool_value = tok.text == "true";
                    Some(expr)
                }
                "null" => {
                    self.index += 1;
                    Some(Expr::new(ExprKind::Null, tok.line, tok.column))
                }
                _ => {
                    self.fail(&format!("expected expression, got keyword '{}'", tok.text));
                    None
                }
            },
            TokenKind::Symbol => match tok.text.as_str() {
                "(" => {
                    self.index += 1;
                    let inner = self.parse_expr()?;
                    self.expect(")", "expected ')' after expression");
                    Some(inner)
                }
                "[" => {
                    self.index += 1;
                    let mut arr = Expr::new(ExprKind::ArrayLiteral, tok.line, tok.column);
                    if !self.match_text("]") {
                        loop {
                            if let Some(item) = self.parse_expr() {
                                arr.args.push(item);
                            }
                            if !self.match_text(",") {
                                break;
                            }
                        }
                        self.expect("]", "expected ']' after array literal");
                    }
                    Some(arr)
                }
                _ => {
                    self.fail(&format!("expected expression, got symbol '{}'", tok.text));
                    None
                }
            },
            TokenKind::Eof => {
                self.fail("expected expression, got end of input");
                None
            }
        }
    }

    /// Parse a type as raw text. Mirrors the type-text approach in
    /// the summary parser; used for the RHS of `as`.
    fn parse_type_text(&mut self) -> String {
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
                    if matches!(
                        t.as_str(),
                        "{" | "}" | "," | "=" | "->" | "=>" | "rescue" | ".."
                    ) {
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
}

fn empty_marker() -> Expr {
    Expr::new(ExprKind::Null, 0, 0)
}

/// Convenience entry point: parse the entire token stream as a single
/// expression. Returns `None` (and fills `diag`) on parse error.
pub fn parse_expression(tokens: &[Token], diag: &mut Diag) -> Option<Expr> {
    let mut parser = ExprParser::new(tokens, diag);
    parser.parse_expr()
}

#[cfg(test)]
mod tests {
    use super::*;
    use zero_lexer::tokenize;

    fn parse_str(s: &str) -> Expr {
        let mut ldiag = Diag::default();
        let tokens = tokenize(s, &mut ldiag);
        assert_eq!(ldiag.code, 0);
        let mut pdiag = Diag::default();
        let expr = parse_expression(&tokens, &mut pdiag).expect("parses");
        assert_eq!(pdiag.code, 0, "expr parse error: {:?}", pdiag);
        expr
    }

    #[test]
    fn ident_expression() {
        let e = parse_str("foo");
        assert_eq!(e.kind, ExprKind::Ident);
        assert_eq!(e.text, "foo");
    }

    #[test]
    fn number_literal() {
        let e = parse_str("42");
        assert_eq!(e.kind, ExprKind::Number);
        assert_eq!(e.text, "42");
    }

    #[test]
    fn binary_with_precedence() {
        // 1 + 2 * 3  ->  1 + (2 * 3)
        let e = parse_str("1 + 2 * 3");
        assert_eq!(e.kind, ExprKind::Binary);
        assert_eq!(e.text, "+");
        assert_eq!(e.left.as_ref().unwrap().text, "1");
        let right = e.right.as_ref().unwrap();
        assert_eq!(right.kind, ExprKind::Binary);
        assert_eq!(right.text, "*");
        assert_eq!(right.left.as_ref().unwrap().text, "2");
        assert_eq!(right.right.as_ref().unwrap().text, "3");
    }

    #[test]
    fn comparison_lower_precedence_than_add() {
        // a + b == c  ->  (a + b) == c
        let e = parse_str("a + b == c");
        assert_eq!(e.kind, ExprKind::Binary);
        assert_eq!(e.text, "==");
        let left = e.left.as_ref().unwrap();
        assert_eq!(left.kind, ExprKind::Binary);
        assert_eq!(left.text, "+");
    }

    #[test]
    fn member_chain() {
        let e = parse_str("world.out.write");
        // outer: (world.out).write
        assert_eq!(e.kind, ExprKind::Member);
        assert_eq!(e.text, "write");
        let inner = e.left.as_ref().unwrap();
        assert_eq!(inner.kind, ExprKind::Member);
        assert_eq!(inner.text, "out");
    }

    #[test]
    fn call_with_args() {
        let e = parse_str("answer()");
        assert_eq!(e.kind, ExprKind::Call);
        assert_eq!(e.args.len(), 0);
        assert_eq!(e.left.as_ref().unwrap().text, "answer");

        let e2 = parse_str("add(1, 2)");
        assert_eq!(e2.kind, ExprKind::Call);
        assert_eq!(e2.args.len(), 2);
    }

    #[test]
    fn member_call_chain() {
        // world.out.write("hello")
        let e = parse_str(r#"world.out.write("hello")"#);
        assert_eq!(e.kind, ExprKind::Call);
        assert_eq!(e.args.len(), 1);
        assert_eq!(e.args[0].kind, ExprKind::String);
        let callee = e.left.as_ref().unwrap();
        assert_eq!(callee.kind, ExprKind::Member);
        assert_eq!(callee.text, "write");
    }

    #[test]
    fn parenthesized_expression() {
        let e = parse_str("(1 + 2) * 3");
        assert_eq!(e.kind, ExprKind::Binary);
        assert_eq!(e.text, "*");
        let left = e.left.as_ref().unwrap();
        assert_eq!(left.kind, ExprKind::Binary);
        assert_eq!(left.text, "+");
    }

    #[test]
    fn borrow_with_mut() {
        let e = parse_str("&mut buf");
        assert_eq!(e.kind, ExprKind::Borrow);
        assert!(e.mutable_borrow);
    }

    #[test]
    fn array_literal() {
        let e = parse_str("[1, 2, 3]");
        assert_eq!(e.kind, ExprKind::ArrayLiteral);
        assert_eq!(e.args.len(), 3);
    }

    #[test]
    fn shape_literal() {
        // Identifier starting with uppercase + `{` triggers shape literal.
        let e = parse_str("Point { x: 1, y: 2 }");
        assert_eq!(e.kind, ExprKind::ShapeLiteral);
        assert_eq!(e.text, "Point");
        assert_eq!(e.fields.len(), 2);
        assert_eq!(e.fields[0].name, "x");
    }

    #[test]
    fn boolean_literals() {
        assert_eq!(parse_str("true").kind, ExprKind::Bool);
        assert!(parse_str("true").bool_value);
        assert!(!parse_str("false").bool_value);
    }

    #[test]
    fn null_literal() {
        assert_eq!(parse_str("null").kind, ExprKind::Null);
    }

    #[test]
    fn cast_expression() {
        let e = parse_str("x as i64");
        assert_eq!(e.kind, ExprKind::Cast);
        assert_eq!(e.text, "i64");
    }

    #[test]
    fn index_expression() {
        let e = parse_str("arr[0]");
        assert_eq!(e.kind, ExprKind::Index);
        assert_eq!(e.right.as_ref().unwrap().kind, ExprKind::Number);
    }
}
