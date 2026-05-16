//! Output normalization rules (RUST_PORT_PLAN.md §5.2).
//!
//! Both the C and Rust compilers' outputs pass through this normalizer before
//! differential comparison. This makes the diff harness ignore machine-specific
//! noise (timings, absolute paths, object-file timestamps, random tokens) while
//! still catching real semantic differences.
//!
//! Contract: any new JSON field added to a `--json` output must be classified
//! in the same PR as either `stable` (kept verbatim) or `masked` (passes
//! through one of the substitutions below). Reviewers reject PRs missing the
//! classification.

use std::sync::OnceLock;

/// Normalize a string blob (typically JSON or plain text from a compiler).
///
/// Order matters: paths first (broadest match), then timing keys, then tokens.
pub fn normalize_text(input: &str) -> String {
    let mut out = input.to_string();
    out = mask_home_path(&out);
    out = mask_tmp_path(&out);
    out = mask_timing_fields(&out);
    out = mask_uuid_like(&out);
    out
}

fn mask_home_path(s: &str) -> String {
    if let Some(home) = home_dir() {
        s.replace(&home, "<HOME>")
    } else {
        s.to_string()
    }
}

fn mask_tmp_path(s: &str) -> String {
    // Common temp prefixes on macOS and Linux. We deliberately do NOT call
    // std::env::temp_dir (banned by clippy.toml); these are the documented
    // POSIX defaults plus macOS's per-user TMPDIR pattern.
    let prefixes = ["/tmp/", "/var/folders/", "/private/var/folders/"];
    let mut out = s.to_string();
    for p in prefixes {
        // Replace the prefix and the following path segment so per-run dirs
        // collapse to a single placeholder.
        out = collapse_after_prefix(&out, p, "<TMP>/");
    }
    out
}

fn collapse_after_prefix(s: &str, prefix: &str, replacement: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(idx) = rest.find(prefix) {
        out.push_str(&rest[..idx]);
        out.push_str(replacement);
        // Skip the prefix, then skip until the next path separator or
        // non-path character so per-run subdirectory names get collapsed.
        let after = &rest[idx + prefix.len()..];
        let skip = after.find(['/', '"', ' ', '\n']).unwrap_or(after.len());
        rest = &after[skip..];
    }
    out.push_str(rest);
    out
}

fn mask_timing_fields(s: &str) -> String {
    // Conservative JSON pattern: match `"name_ms": <number>` (and _seconds, _ns).
    // Replacing with `"name_ms": "<masked>"`. Naive but the C JSON output uses
    // tight `"key": value` formatting so this is enough for Phase 0.
    let suffixes = ["_ms", "_seconds", "_ns"];
    let mut out = s.to_string();
    for suffix in suffixes {
        out = mask_numeric_value_for_suffix(&out, suffix);
    }
    out
}

fn mask_numeric_value_for_suffix(s: &str, key_suffix: &str) -> String {
    let pattern = format!("{}\":", key_suffix);
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(idx) = rest.find(&pattern) {
        // Emit up to and including the `":` so we keep the key intact.
        let after_key = idx + pattern.len();
        out.push_str(&rest[..after_key]);

        // Skip whitespace.
        let mut value_start = after_key;
        while let Some(c) = rest[value_start..].chars().next() {
            if c == ' ' || c == '\t' {
                value_start += c.len_utf8();
            } else {
                break;
            }
        }
        // If the value is already a quoted string, leave it alone.
        if rest[value_start..].starts_with('"') {
            out.push_str(&rest[after_key..]);
            return out;
        }
        // Numeric value: skip digits, optional decimal, optional sign.
        let value_end = rest[value_start..]
            .find(|c: char| c != '-' && c != '+' && c != '.' && !c.is_ascii_digit())
            .map(|n| value_start + n)
            .unwrap_or(rest.len());
        // Consume the original whitespace between `:` and the numeric value so
        // the masked form is consistent regardless of C-side spacing convention.
        let _ = &rest[after_key..value_start];
        out.push_str("\"<masked>\"");
        rest = &rest[value_end..];
    }
    out.push_str(rest);
    out
}

fn mask_uuid_like(s: &str) -> String {
    // Match v4-ish UUIDs: 8-4-4-4-12 hex. Use a byte scan to avoid pulling in
    // a regex dependency at Phase 0.
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if looks_like_uuid(&bytes[i..]) {
            out.push_str("<UUID>");
            i += 36;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

fn looks_like_uuid(b: &[u8]) -> bool {
    if b.len() < 36 {
        return false;
    }
    let groups = [8, 4, 4, 4, 12];
    let mut idx = 0;
    for (g, &len) in groups.iter().enumerate() {
        for _ in 0..len {
            let c = b[idx];
            if !(c.is_ascii_hexdigit()) {
                return false;
            }
            idx += 1;
        }
        if g < groups.len() - 1 {
            if b[idx] != b'-' {
                return false;
            }
            idx += 1;
        }
    }
    true
}

fn home_dir() -> Option<String> {
    static HOME: OnceLock<Option<String>> = OnceLock::new();
    HOME.get_or_init(|| std::env::var("HOME").ok()).clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_ms_timing() {
        let input = r#"{"parse_ms": 42, "check_ms": 1000}"#;
        let out = normalize_text(input);
        assert!(out.contains("\"parse_ms\":\"<masked>\""));
        assert!(out.contains("\"check_ms\":\"<masked>\""));
    }

    #[test]
    fn masks_uuid() {
        let input = "request 550e8400-e29b-41d4-a716-446655440000 done";
        let out = normalize_text(input);
        assert_eq!(out, "request <UUID> done");
    }

    #[test]
    fn collapses_tmp_paths() {
        let input = "/tmp/abc/file.txt and /var/folders/xy/abc/file2";
        let out = normalize_text(input);
        assert!(out.contains("<TMP>/"));
        assert!(!out.contains("/tmp/abc"));
    }

    #[test]
    fn leaves_normal_text_alone() {
        let input = "no machine specific bits here";
        assert_eq!(normalize_text(input), input);
    }
}
