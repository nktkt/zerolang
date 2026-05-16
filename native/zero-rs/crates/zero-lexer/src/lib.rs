//! Tokenizer for the Zero language.
//!
//! Direct port of `native/zero-c/src/lexer.c` (231 lines). The token stream
//! and JSON serialization match the C compiler's `zero tokens --json`
//! schema so the Phase 2 differential test can run both lexers against the
//! same corpus and compare after §5.2 normalization.

use serde::Serialize;
use zero_diag::Diag;

/// Token kind. Lowercase JSON names match the C compiler's `tokens --json`
/// output exactly (see `native/zero-c/src/main.c` token JSON emitter).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenKind {
    Ident,
    Keyword,
    String,
    Char,
    Number,
    Symbol,
    Eof,
}

impl TokenKind {
    pub fn as_str(self) -> &'static str {
        match self {
            TokenKind::Ident => "ident",
            TokenKind::Keyword => "keyword",
            TokenKind::String => "string",
            TokenKind::Char => "char",
            TokenKind::Number => "number",
            TokenKind::Symbol => "symbol",
            TokenKind::Eof => "eof",
        }
    }
}

/// One token. Field names match the JSON schema emitted by the C compiler.
#[derive(Debug, Clone, Serialize)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
    pub line: u32,
    pub column: u32,
    pub offset: usize,
    pub length: usize,
}

/// Reserved keyword list, ordered as in `lexer.c::is_keyword:21-25`.
const KEYWORDS: &[&str] = &[
    "as", "break", "check", "choice", "const", "continue", "defer", "else", "enum", "export",
    "extern", "false", "for", "fun", "if", "import", "in", "let", "match", "meta", "mut", "null",
    "packed", "pub", "raise", "raises", "rescue", "return", "shape", "static", "test", "true",
    "type", "use", "var", "while",
];

/// Two-character operator/punctuator symbols, ordered as in
/// `lexer.c::two_char_symbol:34`.
const TWO_CHAR_SYMBOLS: &[&str] = &[
    "->", "=>", "..", "==", "!=", "<=", ">=", "&&", "||", "+%", "+|",
];

/// Single-character operator/punctuator set, matching `lexer.c:207`.
const SINGLE_CHAR_SYMBOLS: &str = "(){}[],.:<>=+-*/%!&";

fn is_keyword(text: &str) -> bool {
    KEYWORDS.contains(&text)
}

fn matches_two_char_symbol(bytes: &[u8], offset: usize) -> bool {
    if offset + 1 >= bytes.len() {
        return false;
    }
    let a = bytes[offset];
    let b = bytes[offset + 1];
    TWO_CHAR_SYMBOLS
        .iter()
        .any(|s| s.as_bytes()[0] == a && s.as_bytes()[1] == b)
}

fn hex_digit(ch: u8) -> Option<u32> {
    match ch {
        b'0'..=b'9' => Some((ch - b'0') as u32),
        b'a'..=b'f' => Some((ch - b'a' + 10) as u32),
        b'A'..=b'F' => Some((ch - b'A' + 10) as u32),
        _ => None,
    }
}

fn set_char_diag(diag: &mut Diag, line: u32, column: u32, message: &str) {
    diag.code = 3024;
    diag.line = line;
    diag.column = column;
    diag.length = 1;
    diag.message = message.to_string();
    diag.expected = "one byte character literal".to_string();
    diag.help =
        "use a single ASCII byte or an escape like '\\n', '\\\\', '\\'', or '\\x41'".to_string();
}

/// Tokenize a Zero source string.
///
/// On lexical error, fills `diag` and returns the partial token stream
/// gathered up to the error (matching the C lexer's behavior of returning
/// whatever it had on failure).
pub fn tokenize(source: &str, diag: &mut Diag) -> Vec<Token> {
    let bytes = source.as_bytes();
    let mut tokens: Vec<Token> = Vec::with_capacity(64);
    let mut offset: usize = 0;
    let mut line: u32 = 1;
    let mut column: u32 = 1;

    while offset < bytes.len() {
        let ch = bytes[offset];

        // Whitespace, with line/column tracking.
        if (ch as char).is_ascii_whitespace() {
            offset += 1;
            if ch == b'\n' {
                line += 1;
                column = 1;
            } else {
                column += 1;
            }
            continue;
        }

        // Line comment: `// ...` to end of line.
        if ch == b'/' && offset + 1 < bytes.len() && bytes[offset + 1] == b'/' {
            while offset < bytes.len() && bytes[offset] != b'\n' {
                offset += 1;
                column += 1;
            }
            continue;
        }

        let start_line = line;
        let start_column = column;
        let start = offset;

        // Identifier or keyword.
        if (ch as char).is_ascii_alphabetic() || ch == b'_' {
            while offset < bytes.len()
                && ((bytes[offset] as char).is_ascii_alphanumeric() || bytes[offset] == b'_')
            {
                offset += 1;
                column += 1;
            }
            let text = String::from_utf8_lossy(&bytes[start..offset]).to_string();
            let kind = if is_keyword(&text) {
                TokenKind::Keyword
            } else {
                TokenKind::Ident
            };
            tokens.push(Token {
                kind,
                text,
                line: start_line,
                column: start_column,
                offset: start,
                length: offset - start,
            });
            continue;
        }

        // Number literal. Mirrors lexer.c:99-108: digits + alphanumerics +
        // underscores + dots + e-style exponent signs, stopping before `..`.
        if (ch as char).is_ascii_digit() {
            while offset < bytes.len() {
                let c = bytes[offset];
                let is_digit_or_id = (c as char).is_ascii_alphanumeric() || c == b'_';
                let is_dot = c == b'.';
                let is_exponent_sign = (c == b'+' || c == b'-')
                    && offset > start
                    && (bytes[offset - 1] == b'e' || bytes[offset - 1] == b'E');
                if !(is_digit_or_id || is_dot || is_exponent_sign) {
                    break;
                }
                if c == b'.' && offset + 1 < bytes.len() && bytes[offset + 1] == b'.' {
                    break;
                }
                offset += 1;
                column += 1;
            }
            let text = String::from_utf8_lossy(&bytes[start..offset]).to_string();
            tokens.push(Token {
                kind: TokenKind::Number,
                text,
                line: start_line,
                column: start_column,
                offset: start,
                length: offset - start,
            });
            continue;
        }

        // String literal. Mirrors lexer.c:110-137. Escape handling is
        // intentionally simple: '\n' -> newline; any other escape produces
        // the escaped character itself (preserving C parity).
        if ch == b'"' {
            offset += 1;
            column += 1;
            let mut value = String::new();
            while offset < bytes.len() && bytes[offset] != b'"' {
                let next = bytes[offset];
                offset += 1;
                column += 1;
                if next == b'\\' && offset < bytes.len() {
                    let escaped = bytes[offset];
                    offset += 1;
                    column += 1;
                    value.push(if escaped == b'n' { '\n' } else { escaped as char });
                } else {
                    value.push(next as char);
                }
            }
            if offset >= bytes.len() || bytes[offset] != b'"' {
                diag.code = 100;
                diag.line = start_line;
                diag.column = start_column;
                diag.message = "unterminated string literal".to_string();
                return tokens;
            }
            offset += 1;
            column += 1;
            tokens.push(Token {
                kind: TokenKind::String,
                text: value,
                line: start_line,
                column: start_column,
                offset: start,
                length: offset - start,
            });
            continue;
        }

        // Character literal. Mirrors lexer.c:139-198 including all escape
        // forms and the requirement that the literal hold exactly one byte.
        if ch == b'\'' {
            offset += 1;
            column += 1;
            if offset >= bytes.len() || bytes[offset] == b'\n' || bytes[offset] == b'\'' {
                set_char_diag(diag, start_line, start_column, "malformed character literal");
                return tokens;
            }
            let value: u32;
            if bytes[offset] == b'\\' {
                offset += 1;
                column += 1;
                if offset >= bytes.len() || bytes[offset] == b'\n' {
                    set_char_diag(diag, start_line, start_column, "malformed character escape");
                    return tokens;
                }
                let escaped = bytes[offset];
                value = match escaped {
                    b'n' => '\n' as u32,
                    b'r' => '\r' as u32,
                    b't' => '\t' as u32,
                    b'0' => 0,
                    b'\'' => '\'' as u32,
                    b'"' => '"' as u32,
                    b'\\' => '\\' as u32,
                    b'x' => {
                        if offset + 2 >= bytes.len() {
                            set_char_diag(
                                diag,
                                start_line,
                                start_column,
                                "malformed hex character escape",
                            );
                            return tokens;
                        }
                        let high = hex_digit(bytes[offset + 1]);
                        let low = hex_digit(bytes[offset + 2]);
                        match (high, low) {
                            (Some(h), Some(l)) => {
                                offset += 2;
                                column += 2;
                                (h << 4) | l
                            }
                            _ => {
                                set_char_diag(
                                    diag,
                                    start_line,
                                    start_column,
                                    "malformed hex character escape",
                                );
                                return tokens;
                            }
                        }
                    }
                    _ => {
                        set_char_diag(
                            diag,
                            start_line,
                            start_column,
                            "unsupported character escape",
                        );
                        return tokens;
                    }
                };
                offset += 1;
                column += 1;
            } else {
                let byte = bytes[offset];
                if byte >= 128 {
                    set_char_diag(diag, start_line, start_column, "character literal must be one byte");
                    return tokens;
                }
                value = byte as u32;
                offset += 1;
                column += 1;
            }
            if offset >= bytes.len() || bytes[offset] != b'\'' {
                set_char_diag(
                    diag,
                    start_line,
                    start_column,
                    "character literal must contain exactly one byte",
                );
                return tokens;
            }
            offset += 1;
            column += 1;
            // C lexer stores the decimal byte value as token text.
            let text = value.to_string();
            tokens.push(Token {
                kind: TokenKind::Char,
                text,
                line: start_line,
                column: start_column,
                offset: start,
                length: offset - start,
            });
            continue;
        }

        // Two-character symbol.
        if matches_two_char_symbol(bytes, offset) {
            let text = String::from_utf8_lossy(&bytes[offset..offset + 2]).to_string();
            tokens.push(Token {
                kind: TokenKind::Symbol,
                text,
                line: start_line,
                column: start_column,
                offset: start,
                length: 2,
            });
            offset += 2;
            column += 2;
            continue;
        }

        // Single-character symbol.
        if SINGLE_CHAR_SYMBOLS.bytes().any(|b| b == ch) {
            let text = (ch as char).to_string();
            tokens.push(Token {
                kind: TokenKind::Symbol,
                text,
                line: start_line,
                column: start_column,
                offset: start,
                length: 1,
            });
            offset += 1;
            column += 1;
            continue;
        }

        // Unexpected character.
        diag.code = 101;
        diag.line = start_line;
        diag.column = start_column;
        diag.message = format!("unexpected character '{}'", ch as char);
        return tokens;
    }

    tokens.push(Token {
        kind: TokenKind::Eof,
        text: String::new(),
        line,
        column,
        offset,
        length: 0,
    });
    tokens
}

/// Serialize a token stream to JSON matching the C compiler's
/// `tokens --json` output schema:
///
/// ```json
/// {
///   "schemaVersion": 1,
///   "sourceFile": "<path>",
///   "tokens": [ {"kind": "...", "text": "...", "line": N, ...}, ... ]
/// }
/// ```
///
/// The token list includes the trailing EOF marker, matching the C
/// compiler's `tokens --json` output (verified against `bin/zero tokens
/// --json examples/hello.0`).
pub fn tokens_to_json(source_file: &str, tokens: &[Token]) -> String {
    let value = serde_json::json!({
        "schemaVersion": 1,
        "sourceFile": source_file,
        "tokens": tokens,
    });
    serde_json::to_string_pretty(&value).expect("tokens always serialize")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tok(src: &str) -> Vec<Token> {
        let mut diag = Diag::default();
        let tokens = tokenize(src, &mut diag);
        assert_eq!(diag.code, 0, "unexpected diagnostic: {:?}", diag);
        tokens
    }

    fn kinds_and_text(toks: &[Token]) -> Vec<(TokenKind, String)> {
        toks.iter().map(|t| (t.kind, t.text.clone())).collect()
    }

    #[test]
    fn empty_source_yields_eof_only() {
        let toks = tok("");
        assert_eq!(toks.len(), 1);
        assert_eq!(toks[0].kind, TokenKind::Eof);
        assert_eq!(toks[0].line, 1);
        assert_eq!(toks[0].column, 1);
    }

    #[test]
    fn keyword_vs_ident_classification() {
        let toks = tok("pub fun main world");
        let kt = kinds_and_text(&toks);
        assert_eq!(
            kt,
            vec![
                (TokenKind::Keyword, "pub".into()),
                (TokenKind::Keyword, "fun".into()),
                (TokenKind::Ident, "main".into()),
                (TokenKind::Ident, "world".into()),
                (TokenKind::Eof, "".into()),
            ]
        );
    }

    #[test]
    fn all_35_keywords_classify_as_keyword() {
        for kw in KEYWORDS {
            let toks = tok(kw);
            assert_eq!(toks[0].kind, TokenKind::Keyword, "{kw} should be keyword");
        }
    }

    #[test]
    fn two_char_symbols_eat_two_bytes() {
        let toks = tok("-> => .. == != <= >= && || +% +|");
        let symbols: Vec<&str> = toks
            .iter()
            .filter(|t| t.kind == TokenKind::Symbol)
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(
            symbols,
            vec!["->", "=>", "..", "==", "!=", "<=", ">=", "&&", "||", "+%", "+|"]
        );
    }

    #[test]
    fn dot_dot_breaks_number() {
        // `0..3` must tokenize as Number("0"), Symbol(".."), Number("3").
        let toks = tok("0..3");
        let kt = kinds_and_text(&toks);
        assert_eq!(
            kt,
            vec![
                (TokenKind::Number, "0".into()),
                (TokenKind::Symbol, "..".into()),
                (TokenKind::Number, "3".into()),
                (TokenKind::Eof, "".into()),
            ]
        );
    }

    #[test]
    fn number_with_exponent_keeps_sign() {
        let toks = tok("1.5e+10");
        assert_eq!(toks[0].kind, TokenKind::Number);
        assert_eq!(toks[0].text, "1.5e+10");
    }

    #[test]
    fn string_with_newline_escape() {
        let toks = tok(r#""hello\n""#);
        assert_eq!(toks[0].kind, TokenKind::String);
        assert_eq!(toks[0].text, "hello\n");
    }

    #[test]
    fn char_literal_is_decimal_byte_value() {
        let toks = tok("'A'");
        assert_eq!(toks[0].kind, TokenKind::Char);
        assert_eq!(toks[0].text, "65");
    }

    #[test]
    fn char_literal_hex_escape() {
        let toks = tok(r"'\x4a'");
        assert_eq!(toks[0].text, "74");
    }

    #[test]
    fn char_literal_named_escape() {
        for (src, expected) in [
            (r"'\n'", "10"),
            (r"'\r'", "13"),
            (r"'\t'", "9"),
            (r"'\0'", "0"),
            (r"'\\'", "92"),
        ] {
            let toks = tok(src);
            assert_eq!(toks[0].text, expected, "input {src}");
        }
    }

    #[test]
    fn line_comments_skipped() {
        let toks = tok("a // comment to end of line\nb");
        let idents: Vec<&str> = toks
            .iter()
            .filter(|t| t.kind == TokenKind::Ident)
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(idents, vec!["a", "b"]);
        assert_eq!(toks[1].line, 2);
    }

    #[test]
    fn unterminated_string_sets_diag_100() {
        let mut diag = Diag::default();
        let _ = tokenize("\"oops", &mut diag);
        assert_eq!(diag.code, 100);
        assert_eq!(diag.message, "unterminated string literal");
    }

    #[test]
    fn unexpected_char_sets_diag_101() {
        let mut diag = Diag::default();
        let _ = tokenize("@", &mut diag);
        assert_eq!(diag.code, 101);
        assert!(diag.message.contains("unexpected character"));
    }

    #[test]
    fn multi_byte_char_literal_sets_diag_3024() {
        let mut diag = Diag::default();
        let _ = tokenize("'あ'", &mut diag);
        assert_eq!(diag.code, 3024);
    }

    #[test]
    fn hello_world_example_tokenizes() {
        let src = include_str!("../../../../../examples/hello.0");
        let toks = tok(src);
        // First few tokens should be: pub fun main ( world : World )
        let kt: Vec<_> = toks
            .iter()
            .take(8)
            .map(|t| (t.kind, t.text.clone()))
            .collect();
        assert_eq!(
            kt,
            vec![
                (TokenKind::Keyword, "pub".into()),
                (TokenKind::Keyword, "fun".into()),
                (TokenKind::Ident, "main".into()),
                (TokenKind::Symbol, "(".into()),
                (TokenKind::Ident, "world".into()),
                (TokenKind::Symbol, ":".into()),
                (TokenKind::Ident, "World".into()),
                (TokenKind::Symbol, ")".into()),
            ]
        );
    }

    #[test]
    fn json_output_matches_c_schema_for_hello() {
        let src = include_str!("../../../../../examples/hello.0");
        let mut diag = Diag::default();
        let toks = tokenize(src, &mut diag);
        let json = tokens_to_json("examples/hello.0", &toks);
        // Spot-check structural properties; full differential lives in
        // the integration test that runs the C compiler side by side.
        assert!(json.contains("\"schemaVersion\": 1"));
        assert!(json.contains("\"sourceFile\": \"examples/hello.0\""));
        assert!(json.contains("\"kind\": \"keyword\""));
        assert!(json.contains("\"text\": \"pub\""));
        // EOF marker IS in the output, matching C.
        assert!(json.contains("\"kind\": \"eof\""));
    }

    #[test]
    fn offsets_are_monotonic_nondecreasing() {
        let src = include_str!("../../../../../examples/hello.0");
        let toks = tok(src);
        let mut last = 0usize;
        for t in &toks {
            assert!(t.offset >= last, "offset went backwards: {t:?}");
            last = t.offset;
        }
    }
}
