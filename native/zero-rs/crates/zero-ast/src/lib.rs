//! AST types for the Zero language.
//!
//! Mirrors `Expr`, `Stmt`, `Function`, `Shape`, `EnumDecl`, `Choice`,
//! `InterfaceDecl`, `ConstDecl`, `TypeAlias`, `CImport`, `Program` from
//! `native/zero-c/include/zero.h`.
//!
//! Phase 3 scope: the structs needed to drive `zero parse --json` summary
//! output (top-level decl counts and per-function summary). Full
//! expression and statement subtree fields exist as placeholder vectors;
//! Phase 4 (checker) is what forces them to carry real subtree data,
//! and they get fleshed out then.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExprKind {
    Ident,
    String,
    Char,
    Number,
    Bool,
    Null,
    Member,
    Index,
    Slice,
    Call,
    Binary,
    Cast,
    Borrow,
    Check,
    Rescue,
    Meta,
    ShapeLiteral,
    ArrayLiteral,
}

#[derive(Debug, Clone, Serialize)]
pub struct FieldInit {
    pub name: String,
    pub value: Box<Expr>,
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct Expr {
    pub kind: ExprKind,
    /// Identifier text, literal text, operator text, member name,
    /// cast type text, etc. depending on `kind`.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left: Option<Box<Expr>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right: Option<Box<Expr>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<Expr>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<FieldInit>,
    pub bool_value: bool,
    pub mutable_borrow: bool,
    pub line: u32,
    pub column: u32,
}

impl Expr {
    pub fn new(kind: ExprKind, line: u32, column: u32) -> Self {
        Self {
            kind,
            text: String::new(),
            left: None,
            right: None,
            args: Vec::new(),
            fields: Vec::new(),
            bool_value: false,
            mutable_borrow: false,
            line,
            column,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StmtKind {
    Let,
    Assign,
    Defer,
    Check,
    Return,
    Expr,
    If,
    While,
    For,
    Break,
    Continue,
    Match,
    Raise,
}

impl StmtKind {
    pub fn as_str(self) -> &'static str {
        match self {
            StmtKind::Let => "let",
            StmtKind::Assign => "assign",
            StmtKind::Defer => "defer",
            StmtKind::Check => "check",
            StmtKind::Return => "return",
            StmtKind::Expr => "expr",
            StmtKind::If => "if",
            StmtKind::While => "while",
            StmtKind::For => "for",
            StmtKind::Break => "break",
            StmtKind::Continue => "continue",
            StmtKind::Match => "match",
            StmtKind::Raise => "raise",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Stmt {
    pub kind: StmtKind,
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct Param {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct Function {
    pub name: String,
    #[serde(rename = "returnType")]
    pub return_type: String,
    pub params: Vec<Param>,
    pub body: Vec<Stmt>,
    #[serde(rename = "isPublic")]
    pub is_public: bool,
    pub raises: bool,
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct Shape {
    pub name: String,
    pub fields: Vec<Param>,
    pub methods: Vec<Function>,
    #[serde(rename = "isPublic")]
    pub is_public: bool,
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct EnumDecl {
    pub name: String,
    pub cases: Vec<Param>,
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct Choice {
    pub name: String,
    pub cases: Vec<Param>,
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Program {
    pub shapes: Vec<Shape>,
    pub enums: Vec<EnumDecl>,
    pub choices: Vec<Choice>,
    pub functions: Vec<Function>,
}
