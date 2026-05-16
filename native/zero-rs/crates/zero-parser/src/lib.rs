//! Parser for the Zero language.
//!
//! Port of `native/zero-c/src/parser.c` (1,189 lines).
//!
//! Phase 3 scope: produces enough AST to satisfy `zero parse --json`
//! summary output (decl counts, function name/return-type/param-count/
//! body-stmt-kinds, shape field-count/method-count, enum/choice
//! case-count). Body statements are recognized by their first token but
//! their subtrees are NOT built — expression parsing is reduced to
//! token-stream skipping with bracket-depth tracking. The full AST
//! arrives when Phase 4 (checker) requires it.

use zero_ast::{Choice, EnumDecl, Function, Param, Program, Shape, Stmt, StmtKind};
use zero_diag::Diag;
use zero_lexer::{Token, TokenKind};

pub mod expr;
pub mod full;
pub use expr::{parse_expression, ExprParser};
pub use full::{parse_full_program, FullParser};

struct Parser<'a> {
    tokens: &'a [Token],
    index: usize,
    diag: &'a mut Diag,
}

impl<'a> Parser<'a> {
    fn current(&self) -> &Token {
        &self.tokens[self.index.min(self.tokens.len() - 1)]
    }

    fn at_eof(&self) -> bool {
        self.current().kind == TokenKind::Eof
    }

    fn peek_text(&self, offset: usize) -> Option<&str> {
        self.tokens
            .get(self.index + offset)
            .map(|t| t.text.as_str())
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

    fn expect_text(&mut self, text: &str, message: &str) -> bool {
        if self.match_text(text) {
            true
        } else {
            self.fail(message);
            false
        }
    }

    fn fail(&mut self, message: &str) {
        if self.diag.code == 0 {
            let (line, column) = {
                let cur = self.current();
                (cur.line, cur.column)
            };
            self.diag.code = 200;
            self.diag.line = line;
            self.diag.column = column;
            self.diag.message = message.to_string();
        }
    }
}

/// Parse a token stream into a Program. On parse error, fills `diag`
/// and returns whatever decls were successfully parsed.
pub fn parse(tokens: &[Token], diag: &mut Diag) -> Program {
    let mut parser = Parser {
        tokens,
        index: 0,
        diag,
    };
    let mut program = Program::default();
    while !parser.at_eof() && parser.diag.code == 0 {
        // Capture position of the first decl keyword (pub or the bare
        // keyword) so reported line/column matches what the C compiler
        // emits for `parse --json`.
        let decl_line = parser.current().line;
        let decl_col = parser.current().column;
        let is_public = parser.match_text("pub");
        // Strip extern qualifier (rare in surface programs but allowed).
        let _ = parser.match_text("extern");
        if parser.check("import") || parser.check("use") {
            skip_to_next_top_level(&mut parser);
            continue;
        }
        if parser.check("const") {
            skip_const_or_alias(&mut parser);
            continue;
        }
        if parser.check("type") {
            skip_const_or_alias(&mut parser);
            continue;
        }
        if parser.check("test") {
            // Test blocks compile to functions in the C side; we record them
            // as no-op for the summary because parse --json doesn't expose
            // tests separately.
            skip_test_block(&mut parser);
            continue;
        }
        if parser.check("shape") {
            if let Some(mut s) = parse_shape(&mut parser, is_public) {
                s.line = decl_line;
                s.column = decl_col;
                program.shapes.push(s);
            }
            continue;
        }
        if parser.check("enum") {
            if let Some(mut e) = parse_enum(&mut parser) {
                e.line = decl_line;
                e.column = decl_col;
                program.enums.push(e);
            }
            continue;
        }
        if parser.check("choice") {
            if let Some(mut c) = parse_choice(&mut parser) {
                c.line = decl_line;
                c.column = decl_col;
                program.choices.push(c);
            }
            continue;
        }
        if parser.check("interface") {
            // Interfaces have methods that look like functions but aren't
            // in the functions array. Skip the whole block.
            skip_brace_construct(&mut parser);
            continue;
        }
        if parser.check("fun") {
            if let Some(mut f) = parse_function(&mut parser, is_public) {
                f.line = decl_line;
                f.column = decl_col;
                program.functions.push(f);
            }
            continue;
        }
        // Unknown top-level token; advance to avoid infinite loop.
        parser.fail(&format!(
            "unexpected top-level token '{}'",
            parser.current().text
        ));
    }
    program
}

fn parse_function(parser: &mut Parser<'_>, is_public: bool) -> Option<Function> {
    let fun_tok_line = parser.current().line;
    let fun_tok_col = parser.current().column;
    parser.expect_text("fun", "expected 'fun'");
    let name_tok = parser.current().clone();
    if name_tok.kind != TokenKind::Ident {
        parser.fail("expected function name");
        return None;
    }
    parser.index += 1;

    // Optional type params <T, U>.
    if parser.check("<") {
        skip_balanced(parser, "<", ">");
    }

    let params = parse_param_list(parser);
    let return_type = if parser.match_text("->") {
        parse_type_text(parser)
    } else {
        String::new()
    };
    let raises = parser.match_text("raises");
    if raises && parser.check("(") {
        skip_balanced(parser, "(", ")");
    }

    // Body
    let body = if parser.check("{") {
        parse_function_body(parser)
    } else {
        // Forward decl (no body).
        Vec::new()
    };

    Some(Function {
        name: name_tok.text,
        return_type,
        params,
        body,
        is_public,
        raises,
        line: fun_tok_line,
        column: fun_tok_col,
    })
}

fn parse_param_list(parser: &mut Parser<'_>) -> Vec<Param> {
    let mut params = Vec::new();
    if !parser.match_text("(") {
        return params;
    }
    while !parser.check(")") && !parser.at_eof() && parser.diag.code == 0 {
        // Optional `static` keyword in front.
        let _ = parser.match_text("static");
        let name_tok = parser.current().clone();
        if name_tok.kind != TokenKind::Ident {
            parser.fail("expected parameter name");
            break;
        }
        parser.index += 1;
        let mut ty = String::new();
        if parser.match_text(":") {
            ty = parse_type_text(parser);
        }
        // Skip optional default value `= expr`.
        if parser.match_text("=") {
            skip_until_statement_or_paren_close(parser);
        }
        params.push(Param {
            name: name_tok.text,
            ty,
            line: name_tok.line,
            column: name_tok.column,
        });
        if !parser.match_text(",") {
            break;
        }
    }
    parser.expect_text(")", "expected ')' after parameters");
    params
}

/// Parse a type as raw text. Mirrors C `parse_type` which builds a string
/// like `Vec<U8>` or `Maybe<&[U8]>`.
fn parse_type_text(parser: &mut Parser<'_>) -> String {
    let mut out = String::new();
    let mut depth_angle = 0i32;
    let mut depth_paren = 0i32;
    let mut depth_bracket = 0i32;
    loop {
        let tok = parser.current();
        if tok.kind == TokenKind::Eof {
            break;
        }
        let t = tok.text.as_str();
        if t == "<" {
            depth_angle += 1;
        } else if t == ">" {
            if depth_angle == 0 {
                break;
            }
            depth_angle -= 1;
        } else if t == "(" {
            depth_paren += 1;
        } else if t == ")" {
            if depth_paren == 0 {
                break;
            }
            depth_paren -= 1;
        } else if t == "[" {
            depth_bracket += 1;
        } else if t == "]" {
            if depth_bracket == 0 {
                break;
            }
            depth_bracket -= 1;
        } else if depth_angle == 0 && depth_paren == 0 && depth_bracket == 0 {
            // Stop at tokens that can't be part of a type.
            if matches!(
                t,
                "{" | "}" | ";" | "->" | "=>" | "raises" | "," | "=" | "in" | ".." | ":"
            ) {
                // ',' inside angle/paren brackets is part of the type;
                // top-level ',' / ':' / ... are not.
                if t == "," && (depth_angle > 0 || depth_paren > 0 || depth_bracket > 0) {
                    // unreachable due to outer condition, kept for clarity
                } else {
                    break;
                }
            }
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(t);
        parser.index += 1;
    }
    // Compact common patterns: "Vec < U8 >" -> "Vec<U8>"
    compact_type_text(&out)
}

fn compact_type_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c == ' ' {
            // Skip space if neighbor is a punctuator/bracket that the
            // C compiler would not separate. Heuristic only; the parse
            // --json schema only carries this string in error messages.
            let prev = if !out.is_empty() {
                out.as_bytes()[out.len() - 1] as char
            } else {
                ' '
            };
            let next = bytes.get(i + 1).map(|&b| b as char).unwrap_or(' ');
            if "<>(,)[]".contains(prev) || "<>(,)[]".contains(next) {
                i += 1;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Parse the body of a function/method, returning Stmt entries (one per
/// top-level statement). The opening `{` is consumed; we stop at the
/// matching `}`.
fn parse_function_body(parser: &mut Parser<'_>) -> Vec<Stmt> {
    let mut stmts = Vec::new();
    if !parser.match_text("{") {
        return stmts;
    }
    while !parser.check("}") && !parser.at_eof() && parser.diag.code == 0 {
        let start_line = parser.current().line;
        let start_col = parser.current().column;
        let kind = classify_statement(parser);
        stmts.push(Stmt::new(kind, start_line, start_col));
        skip_one_statement(parser, kind);
    }
    parser.expect_text("}", "expected '}' after block");
    stmts
}

/// Identify the statement kind from the current token (and a small
/// amount of lookahead for the ident-assign case).
fn classify_statement(parser: &Parser<'_>) -> StmtKind {
    let cur = parser.current();
    if cur.kind == TokenKind::Keyword {
        match cur.text.as_str() {
            "let" => return StmtKind::Let,
            "defer" => return StmtKind::Defer,
            "check" => return StmtKind::Check,
            "return" => return StmtKind::Return,
            "raise" => return StmtKind::Raise,
            "if" => return StmtKind::If,
            "while" => return StmtKind::While,
            "for" => return StmtKind::For,
            "break" => return StmtKind::Break,
            "continue" => return StmtKind::Continue,
            "match" => return StmtKind::Match,
            _ => {}
        }
    }
    if cur.kind == TokenKind::Ident && lookahead_is_assign(parser) {
        return StmtKind::Assign;
    }
    StmtKind::Expr
}

/// Walk forward from `parser.index` over a postfix chain (`.name`,
/// `[index]`, `(args)`) and return true if the next non-chain token is
/// `=` (i.e. assignment target).
fn lookahead_is_assign(parser: &Parser<'_>) -> bool {
    let mut i = parser.index;
    if parser.tokens[i].kind != TokenKind::Ident {
        return false;
    }
    i += 1;
    loop {
        if i >= parser.tokens.len() {
            return false;
        }
        let t = parser.tokens[i].text.as_str();
        match t {
            "." => {
                i += 1;
                if i >= parser.tokens.len() || parser.tokens[i].kind != TokenKind::Ident {
                    return false;
                }
                i += 1;
            }
            "(" => {
                let close = find_matching(parser.tokens, i, "(", ")");
                let Some(close) = close else { return false };
                i = close + 1;
            }
            "[" => {
                let close = find_matching(parser.tokens, i, "[", "]");
                let Some(close) = close else { return false };
                i = close + 1;
            }
            "=" => return true,
            "==" => return false, // comparison, not assign
            _ => return false,
        }
    }
}

fn find_matching(tokens: &[Token], open_idx: usize, open: &str, close: &str) -> Option<usize> {
    let mut depth = 1i32;
    let mut i = open_idx + 1;
    while i < tokens.len() {
        let t = tokens[i].text.as_str();
        if t == open {
            depth += 1;
        } else if t == close {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

/// Consume one statement starting at parser.index. The statement's kind
/// is already known; we just walk past its end.
fn skip_one_statement(parser: &mut Parser<'_>, kind: StmtKind) {
    match kind {
        StmtKind::If | StmtKind::While => {
            parser.index += 1; // consume keyword
            skip_until_brace(parser);
            skip_brace_block(parser);
            if parser.match_text("else") {
                if parser.check("if") {
                    // else if -> recurse as a fresh if
                    skip_one_statement(parser, StmtKind::If);
                } else {
                    skip_brace_block(parser);
                }
            }
        }
        StmtKind::For => {
            parser.index += 1;
            skip_until_brace(parser);
            skip_brace_block(parser);
        }
        StmtKind::Match => {
            parser.index += 1;
            skip_until_brace(parser);
            skip_brace_block(parser);
        }
        StmtKind::Break | StmtKind::Continue => {
            parser.index += 1;
        }
        _ => {
            // let / assign / defer / check / return / raise / expr
            skip_to_next_stmt_boundary(parser);
        }
    }
}

/// Skip until we hit `{` at depth 0, consuming everything in between.
fn skip_until_brace(parser: &mut Parser<'_>) {
    let mut depth = 0i32;
    while !parser.at_eof() {
        let t = parser.current().text.as_str();
        if depth == 0 && t == "{" {
            return;
        }
        match t {
            "(" | "[" => depth += 1,
            ")" | "]" => depth -= 1,
            _ => {}
        }
        parser.index += 1;
    }
}

/// Consume `{ ... }` block with depth tracking.
fn skip_brace_block(parser: &mut Parser<'_>) {
    if !parser.match_text("{") {
        return;
    }
    let mut depth = 1i32;
    while !parser.at_eof() && depth > 0 {
        let t = parser.current().text.as_str();
        if t == "{" {
            depth += 1;
        } else if t == "}" {
            depth -= 1;
            if depth == 0 {
                parser.index += 1;
                return;
            }
        }
        parser.index += 1;
    }
}

/// Skip an expression-only statement: walk forward at depth 0 until we
/// hit `}` (end of containing block) or the start of the next statement.
///
/// We do NOT use ident-then-`=` lookahead here because the assignment
/// operator is not a valid expression operator in Zero — if `=` appears
/// inside the RHS of a let/check/return/etc., it would be `==` or part
/// of `=>` (and those tokenize as their own symbols). So statement-
/// starting keywords plus `}` are sufficient boundaries.
fn skip_to_next_stmt_boundary(parser: &mut Parser<'_>) {
    let mut depth = 0i32;
    let mut consumed_any = false;
    while !parser.at_eof() {
        let cur_text = parser.current().text.to_string();
        let cur_kind = parser.current().kind;
        if depth == 0 {
            if cur_text == "}" {
                return;
            }
            if consumed_any && is_statement_starter(cur_kind, &cur_text) {
                return;
            }
        }
        match cur_text.as_str() {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => {
                if depth == 0 {
                    return;
                }
                depth -= 1;
            }
            _ => {}
        }
        parser.index += 1;
        consumed_any = true;
    }
}

fn is_statement_starter(kind: TokenKind, text: &str) -> bool {
    if kind == TokenKind::Keyword {
        matches!(
            text,
            "let"
                | "defer"
                | "check"
                | "return"
                | "raise"
                | "if"
                | "while"
                | "for"
                | "break"
                | "continue"
                | "match"
        )
    } else {
        false
    }
}

/// Like skip_to_next_stmt_boundary but also stops on `)` at depth 0 —
/// used inside parameter default-value parsing.
fn skip_until_statement_or_paren_close(parser: &mut Parser<'_>) {
    let mut depth = 0i32;
    while !parser.at_eof() {
        let t = parser.current().text.as_str();
        if depth == 0 && (t == "," || t == ")") {
            return;
        }
        match t {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => {
                if depth == 0 {
                    return;
                }
                depth -= 1;
            }
            _ => {}
        }
        parser.index += 1;
    }
}

fn parse_shape(parser: &mut Parser<'_>, is_public: bool) -> Option<Shape> {
    let line = parser.current().line;
    let column = parser.current().column;
    parser.expect_text("shape", "expected 'shape'");
    let name_tok = parser.current().clone();
    if name_tok.kind != TokenKind::Ident {
        parser.fail("expected shape name");
        return None;
    }
    parser.index += 1;

    // Optional type params.
    if parser.check("<") {
        skip_balanced(parser, "<", ">");
    }
    // Optional layout tag like `packed`.
    let _ = parser.match_text("packed");

    let mut fields: Vec<Param> = Vec::new();
    let mut methods: Vec<Function> = Vec::new();
    if parser.match_text("{") {
        while !parser.check("}") && !parser.at_eof() && parser.diag.code == 0 {
            // Strip leading `pub` on field/method visibility.
            let inner_pub = parser.match_text("pub");
            if parser.check("fun") {
                if let Some(f) = parse_function(parser, inner_pub) {
                    methods.push(f);
                }
            } else {
                // Field: name : type
                let nm = parser.current().clone();
                if nm.kind != TokenKind::Ident {
                    parser.fail("expected field name");
                    break;
                }
                parser.index += 1;
                let mut ty = String::new();
                if parser.match_text(":") {
                    ty = parse_type_text(parser);
                }
                if parser.match_text("=") {
                    skip_until_statement_or_paren_close(parser);
                }
                fields.push(Param {
                    name: nm.text,
                    ty,
                    line: nm.line,
                    column: nm.column,
                });
                let _ = parser.match_text(",");
            }
        }
        parser.expect_text("}", "expected '}' after shape body");
    }

    Some(Shape {
        name: name_tok.text,
        fields,
        methods,
        is_public,
        line,
        column,
    })
}

fn parse_enum(parser: &mut Parser<'_>) -> Option<EnumDecl> {
    let line = parser.current().line;
    let column = parser.current().column;
    parser.expect_text("enum", "expected 'enum'");
    let name_tok = parser.current().clone();
    if name_tok.kind != TokenKind::Ident {
        parser.fail("expected enum name");
        return None;
    }
    parser.index += 1;
    // Optional base type `: i32`
    if parser.match_text(":") {
        let _ = parse_type_text(parser);
    }
    let mut cases: Vec<Param> = Vec::new();
    if parser.match_text("{") {
        while !parser.check("}") && !parser.at_eof() && parser.diag.code == 0 {
            let nm = parser.current().clone();
            if nm.kind != TokenKind::Ident {
                parser.fail("expected enum case name");
                break;
            }
            parser.index += 1;
            if parser.match_text("=") {
                skip_until_statement_or_paren_close(parser);
            }
            cases.push(Param {
                name: nm.text,
                ty: String::new(),
                line: nm.line,
                column: nm.column,
            });
            let _ = parser.match_text(",");
        }
        parser.expect_text("}", "expected '}' after enum body");
    }
    Some(EnumDecl {
        name: name_tok.text,
        cases,
        line,
        column,
    })
}

fn parse_choice(parser: &mut Parser<'_>) -> Option<Choice> {
    let line = parser.current().line;
    let column = parser.current().column;
    parser.expect_text("choice", "expected 'choice'");
    let name_tok = parser.current().clone();
    if name_tok.kind != TokenKind::Ident {
        parser.fail("expected choice name");
        return None;
    }
    parser.index += 1;
    let mut cases: Vec<Param> = Vec::new();
    if parser.match_text("{") {
        while !parser.check("}") && !parser.at_eof() && parser.diag.code == 0 {
            let nm = parser.current().clone();
            if nm.kind != TokenKind::Ident {
                parser.fail("expected choice case name");
                break;
            }
            parser.index += 1;
            let mut ty = String::new();
            // Cases may carry a payload type `(T)` or just be a tag.
            if parser.check("(") {
                let open = parser.index;
                if let Some(close) = find_matching(parser.tokens, open, "(", ")") {
                    let inner: Vec<&str> = parser.tokens[open + 1..close]
                        .iter()
                        .map(|t| t.text.as_str())
                        .collect();
                    ty = compact_type_text(&inner.join(" "));
                    parser.index = close + 1;
                }
            }
            cases.push(Param {
                name: nm.text,
                ty,
                line: nm.line,
                column: nm.column,
            });
            let _ = parser.match_text(",");
        }
        parser.expect_text("}", "expected '}' after choice body");
    }
    Some(Choice {
        name: name_tok.text,
        cases,
        line,
        column,
    })
}

fn skip_const_or_alias(parser: &mut Parser<'_>) {
    // `const NAME [: TYPE] = expr`  or  `type NAME = TYPE`
    parser.index += 1; // consume `const` / `type`
    if parser.current().kind == TokenKind::Ident {
        parser.index += 1;
    }
    if parser.match_text(":") {
        let _ = parse_type_text(parser);
    }
    if parser.match_text("=") {
        skip_to_next_stmt_boundary(parser);
    }
}

fn skip_test_block(parser: &mut Parser<'_>) {
    parser.index += 1; // consume `test`
    // Test name is a string literal in C; for the summary parser we just
    // skip until the body.
    while !parser.check("{") && !parser.at_eof() {
        parser.index += 1;
    }
    skip_brace_block(parser);
}

fn skip_brace_construct(parser: &mut Parser<'_>) {
    while !parser.check("{") && !parser.at_eof() {
        parser.index += 1;
    }
    skip_brace_block(parser);
}

fn skip_to_next_top_level(parser: &mut Parser<'_>) {
    // For `use` / `import` lines: consume until we hit something that
    // starts a new top-level decl. Imports don't span newlines so this
    // is conservative.
    while !parser.at_eof() {
        let t = parser.current().text.as_str();
        if matches!(
            t,
            "pub" | "fun" | "shape" | "enum" | "choice" | "const" | "type" | "interface" | "test" | "use" | "import"
        ) && parser.index > 0
        {
            return;
        }
        parser.index += 1;
        if parser.current().kind == TokenKind::Keyword
            && parser.peek_text(0).is_some_and(|s| {
                matches!(
                    s,
                    "pub" | "fun" | "shape" | "enum" | "choice" | "const" | "type" | "interface" | "test"
                )
            })
        {
            return;
        }
    }
}

/// Consume tokens balanced between `open` and `close`, e.g. `<...>`.
fn skip_balanced(parser: &mut Parser<'_>, open: &str, close: &str) {
    if !parser.match_text(open) {
        return;
    }
    let mut depth = 1i32;
    while !parser.at_eof() && depth > 0 {
        let t = parser.current().text.as_str();
        if t == open {
            depth += 1;
        } else if t == close {
            depth -= 1;
            if depth == 0 {
                parser.index += 1;
                return;
            }
        }
        parser.index += 1;
    }
}

/// Serialize a Program to JSON matching the C compiler's
/// `parse --json` output schema.
pub fn parse_to_json(source_file: &str, program: &Program) -> String {
    let mut shapes = serde_json::Value::Array(vec![]);
    if let serde_json::Value::Array(ref mut arr) = shapes {
        for s in &program.shapes {
            arr.push(serde_json::json!({
                "kind": "shape",
                "name": s.name,
                "fieldCount": s.fields.len(),
                "methodCount": s.methods.len(),
                "line": s.line,
                "column": s.column,
            }));
        }
    }
    let mut enums = serde_json::Value::Array(vec![]);
    if let serde_json::Value::Array(ref mut arr) = enums {
        for e in &program.enums {
            arr.push(serde_json::json!({
                "kind": "enum",
                "name": e.name,
                "caseCount": e.cases.len(),
                "line": e.line,
                "column": e.column,
            }));
        }
    }
    let mut choices = serde_json::Value::Array(vec![]);
    if let serde_json::Value::Array(ref mut arr) = choices {
        for c in &program.choices {
            arr.push(serde_json::json!({
                "kind": "choice",
                "name": c.name,
                "caseCount": c.cases.len(),
                "line": c.line,
                "column": c.column,
            }));
        }
    }
    let mut funcs = serde_json::Value::Array(vec![]);
    if let serde_json::Value::Array(ref mut arr) = funcs {
        for f in &program.functions {
            let body_kinds: Vec<&str> = f.body.iter().map(|s| s.kind.as_str()).collect();
            arr.push(serde_json::json!({
                "kind": "function",
                "name": f.name,
                "returnType": f.return_type,
                "paramCount": f.params.len(),
                "bodyKinds": body_kinds,
                "line": f.line,
                "column": f.column,
            }));
        }
    }
    let value = serde_json::json!({
        "schemaVersion": 1,
        "sourceFile": source_file,
        "root": {
            "kind": "module",
            "shapeCount": program.shapes.len(),
            "enumCount": program.enums.len(),
            "choiceCount": program.choices.len(),
            "functionCount": program.functions.len(),
        },
        "shapes": shapes,
        "enums": enums,
        "choices": choices,
        "functions": funcs,
    });
    serde_json::to_string_pretty(&value).expect("parse JSON serializes")
}

#[cfg(test)]
mod tests {
    use super::*;
    use zero_lexer::tokenize;

    fn parse_source(src: &str) -> Program {
        let mut diag = Diag::default();
        let tokens = tokenize(src, &mut diag);
        assert_eq!(diag.code, 0, "lex error: {:?}", diag);
        let mut diag2 = Diag::default();
        let program = parse(&tokens, &mut diag2);
        assert_eq!(diag2.code, 0, "parse error: {:?}", diag2);
        program
    }

    #[test]
    fn empty_program_is_default() {
        let p = parse_source("");
        assert_eq!(p.functions.len(), 0);
        assert_eq!(p.shapes.len(), 0);
    }

    #[test]
    fn hello_world_one_function_check_body() {
        let src = include_str!("../../../../../examples/hello.0");
        let p = parse_source(src);
        assert_eq!(p.functions.len(), 1);
        let f = &p.functions[0];
        assert_eq!(f.name, "main");
        assert_eq!(f.return_type, "Void");
        assert_eq!(f.params.len(), 1);
        assert!(f.is_public);
        assert!(f.raises);
        let kinds: Vec<&str> = f.body.iter().map(|s| s.kind.as_str()).collect();
        assert_eq!(kinds, vec!["check"]);
    }

    #[test]
    fn add_two_functions_with_correct_body_kinds() {
        let src = include_str!("../../../../../examples/add.0");
        let p = parse_source(src);
        assert_eq!(p.functions.len(), 2);

        let answer = &p.functions[0];
        assert_eq!(answer.name, "answer");
        assert_eq!(answer.return_type, "i32");
        assert_eq!(answer.params.len(), 0);
        assert!(!answer.is_public);
        let kinds: Vec<&str> = answer.body.iter().map(|s| s.kind.as_str()).collect();
        assert_eq!(kinds, vec!["return"]);

        let main = &p.functions[1];
        assert_eq!(main.name, "main");
        assert_eq!(main.return_type, "Void");
        assert!(main.raises);
        let kinds: Vec<&str> = main.body.iter().map(|s| s.kind.as_str()).collect();
        assert_eq!(kinds, vec!["let", "if"]);
    }

    #[test]
    fn branch_example_kinds() {
        let src = include_str!("../../../../../examples/branch.0");
        let p = parse_source(src);
        assert_eq!(p.functions.len(), 1);
        let kinds: Vec<&str> = p.functions[0].body.iter().map(|s| s.kind.as_str()).collect();
        assert_eq!(kinds, vec!["let", "if"]);
    }

    #[test]
    fn parse_to_json_matches_expected_schema() {
        let src = include_str!("../../../../../examples/hello.0");
        let p = parse_source(src);
        let json = parse_to_json("examples/hello.0", &p);
        for expected in [
            "\"schemaVersion\": 1",
            "\"sourceFile\": \"examples/hello.0\"",
            "\"kind\": \"module\"",
            "\"functionCount\": 1",
            "\"name\": \"main\"",
            "\"returnType\": \"Void\"",
            "\"paramCount\": 1",
            "\"check\"",
        ] {
            assert!(json.contains(expected), "missing {expected} in:\n{json}");
        }
    }
}
