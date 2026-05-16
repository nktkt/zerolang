//! Minimal WebAssembly emitter — implements the subset needed to
//! compile constant-returning Zero functions to a valid `.wasm`
//! module.
//!
//! Written from the public WebAssembly Core Specification (W3C
//! recommendation, https://webassembly.github.io/spec/core/binary/).
//! Not derived from any third-party implementation.
//!
//! Phase 6 scope (this PR): a single function of shape `fun NAME()
//! -> i32 { return CONST }`, exported under its declared name.
//! Multi-function modules, parameters, control flow, memory, imports,
//! and table sections are out of scope for this initial emitter and
//! arrive in subsequent PRs.

use zero_ast::{ExprKind, Function, Program, StmtKind};

/// Errors the emitter can produce. Kept tiny because Phase 6 is
/// minimal scope.
#[derive(Debug, thiserror::Error)]
pub enum EmitError {
    #[error("empty program: no functions to emit")]
    EmptyProgram,
    #[error("function '{0}' has parameters; only no-arg functions supported in this Phase 6 slice")]
    HasParameters(String),
    #[error("function '{0}' return type '{1}' is not yet supported (only i32)")]
    UnsupportedReturnType(String, String),
    #[error("function '{0}' body must be a single `return <int literal>` statement in this Phase 6 slice")]
    UnsupportedBody(String),
}

// Encode an unsigned LEB128 integer (used for WASM section sizes,
// counts, indices, and i32.const operands). See WebAssembly spec
// §5.2.2 "Integers".
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

// Encode a signed LEB128 integer (i32.const operand is signed).
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

// Write a length-prefixed byte slice (used for name strings and
// section payloads).
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

/// Extract the int literal that a one-stmt `return N` body returns.
/// Returns None for any other shape.
fn extract_constant_return(f: &Function) -> Option<i64> {
    if f.body.len() != 1 {
        return None;
    }
    let stmt = &f.body[0];
    if stmt.kind != StmtKind::Return {
        return None;
    }
    let expr = stmt.expr.as_ref()?;
    if !matches!(expr.kind, ExprKind::Number) {
        return None;
    }
    expr.text.parse::<i64>().ok()
}

/// Emit a complete `.wasm` module for the given Zero `Function`.
///
/// Constraints: no parameters, return type `i32`, body is one
/// `return <int literal>` statement.
pub fn emit_constant_function(f: &Function) -> Result<Vec<u8>, EmitError> {
    if !f.params.is_empty() {
        return Err(EmitError::HasParameters(f.name.clone()));
    }
    if f.return_type != "i32" {
        return Err(EmitError::UnsupportedReturnType(
            f.name.clone(),
            f.return_type.clone(),
        ));
    }
    let value = extract_constant_return(f).ok_or_else(|| EmitError::UnsupportedBody(f.name.clone()))?;

    let mut module = Vec::with_capacity(128);
    // Magic + version: `\0asm` then 0x01000000.
    module.extend_from_slice(&[0x00, 0x61, 0x73, 0x6D]);
    module.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]);

    // Type section (id 1): one func type `() -> i32`.
    let mut types = Vec::new();
    write_unsigned_leb128(&mut types, 1); // 1 type
    types.push(0x60); // func
    write_unsigned_leb128(&mut types, 0); // 0 params
    write_unsigned_leb128(&mut types, 1); // 1 result
    types.push(0x7F); // i32
    module.extend(make_section(1, &types));

    // Function section (id 3): one function, type index 0.
    let mut funcs = Vec::new();
    write_unsigned_leb128(&mut funcs, 1);
    write_unsigned_leb128(&mut funcs, 0);
    module.extend(make_section(3, &funcs));

    // Export section (id 7): the function as `<name>`, kind func, index 0.
    let mut exports = Vec::new();
    write_unsigned_leb128(&mut exports, 1);
    write_name(&mut exports, &f.name);
    exports.push(0x00); // export kind = func
    write_unsigned_leb128(&mut exports, 0); // func index 0
    module.extend(make_section(7, &exports));

    // Code section (id 10): one body.
    let mut body = Vec::new();
    write_unsigned_leb128(&mut body, 0); // 0 local groups
    body.push(0x41); // i32.const
    write_signed_leb128(&mut body, value);
    body.push(0x0B); // end

    let mut code = Vec::new();
    write_unsigned_leb128(&mut code, 1); // 1 body
    write_unsigned_leb128(&mut code, body.len() as u64);
    code.extend(body);
    module.extend(make_section(10, &code));

    Ok(module)
}

/// Emit the first eligible function in `program`.
pub fn emit_program(program: &Program) -> Result<Vec<u8>, EmitError> {
    let f = program.functions.first().ok_or(EmitError::EmptyProgram)?;
    emit_constant_function(f)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zero_ast::{Expr, Stmt};

    fn make_const_fun(name: &str, value: i64) -> Function {
        let mut return_expr = Expr::new(ExprKind::Number, 1, 1);
        return_expr.text = value.to_string();
        let mut return_stmt = Stmt::new(StmtKind::Return, 1, 1);
        return_stmt.expr = Some(Box::new(return_expr));
        Function {
            name: name.into(),
            return_type: "i32".into(),
            params: Vec::new(),
            body: vec![return_stmt],
            is_public: true,
            raises: false,
            line: 1,
            column: 1,
        }
    }

    #[test]
    fn emits_valid_wasm_magic_and_version() {
        let f = make_const_fun("answer", 42);
        let bytes = emit_constant_function(&f).unwrap();
        assert_eq!(&bytes[0..4], b"\0asm");
        assert_eq!(&bytes[4..8], &[0x01, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn contains_export_name() {
        let f = make_const_fun("answer", 42);
        let bytes = emit_constant_function(&f).unwrap();
        // The export name "answer" must appear as ASCII somewhere in
        // the export section.
        let needle = b"answer";
        let mut found = false;
        for window in bytes.windows(needle.len()) {
            if window == needle {
                found = true;
                break;
            }
        }
        assert!(found, "export name 'answer' should appear in module bytes");
    }

    #[test]
    fn rejects_parameters() {
        let mut f = make_const_fun("greet", 0);
        f.params.push(zero_ast::Param {
            name: "x".into(),
            ty: "i32".into(),
            line: 1,
            column: 1,
        });
        assert!(matches!(
            emit_constant_function(&f),
            Err(EmitError::HasParameters(_))
        ));
    }

    #[test]
    fn rejects_non_i32_return() {
        let mut f = make_const_fun("answer", 1);
        f.return_type = "i64".into();
        assert!(matches!(
            emit_constant_function(&f),
            Err(EmitError::UnsupportedReturnType(_, _))
        ));
    }

    #[test]
    fn rejects_complex_body() {
        let mut f = make_const_fun("answer", 1);
        f.body.push(Stmt::new(StmtKind::Expr, 2, 1));
        assert!(matches!(
            emit_constant_function(&f),
            Err(EmitError::UnsupportedBody(_))
        ));
    }

    #[test]
    fn leb128_zero() {
        let mut v = Vec::new();
        write_unsigned_leb128(&mut v, 0);
        assert_eq!(v, vec![0x00]);
    }

    #[test]
    fn leb128_127_is_single_byte() {
        let mut v = Vec::new();
        write_unsigned_leb128(&mut v, 127);
        assert_eq!(v, vec![0x7F]);
    }

    #[test]
    fn leb128_128_is_two_bytes() {
        let mut v = Vec::new();
        write_unsigned_leb128(&mut v, 128);
        assert_eq!(v, vec![0x80, 0x01]);
    }

    #[test]
    fn signed_leb128_handles_negative() {
        // -1 in signed LEB128 is 0x7F (one byte).
        let mut v = Vec::new();
        write_signed_leb128(&mut v, -1);
        assert_eq!(v, vec![0x7F]);
    }

    #[test]
    fn signed_leb128_42() {
        // 42 is in range [-64, 63] so fits in one signed LEB128 byte.
        let mut v = Vec::new();
        write_signed_leb128(&mut v, 42);
        assert_eq!(v, vec![0x2A]);
    }

    #[test]
    fn whole_module_size_is_reasonable() {
        // Sanity: a minimal module with one no-arg i32 const function
        // should be under 64 bytes.
        let f = make_const_fun("answer", 42);
        let bytes = emit_constant_function(&f).unwrap();
        assert!(bytes.len() < 64, "module too large: {} bytes", bytes.len());
        // And greater than the bare header (8 bytes) since we have
        // type/function/export/code sections.
        assert!(bytes.len() > 8);
    }
}
