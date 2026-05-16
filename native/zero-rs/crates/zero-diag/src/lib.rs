//! Diagnostic types and code table for the Zero compiler.
//!
//! Mirrors `ZDiag` from `native/zero-c/include/zero.h` and the
//! diagnostic code-text table from `native/zero-c/src/main.c::diag_code`
//! (lines 74-157, ~80 codes spanning ERR, APP, BLD, NAM, TYP, OWN,
//! MEM, BOR, ABI, MET, PUB, IFC, STC, SHM, RCV, FLD, VAR, MAT, CGEN,
//! WEB, TAR, IMP, CIMP, PKG, PAR families).

use serde::Serialize;

/// Diagnostic record. Field names match the C JSON output schema so that
/// `serde_json::to_string` produces output the differential harness can
/// compare against the C compiler's output after normalization (§5.2).
///
/// Mirrors `ZDiag` in `native/zero-c/include/zero.h:13-24`.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diag {
    pub code: u32,
    pub message: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub expected: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub actual: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub help: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub path: String,
    pub line: u32,
    pub column: u32,
    pub length: u32,
}

impl Diag {
    /// Returns the diagnostic's text code (e.g. "ERR001", "TYP009").
    pub fn code_text(&self) -> &'static str {
        diag_code(self.code)
    }
}

/// Map a numeric diagnostic code to its string code.
///
/// Mirrors `diag_code` in `native/zero-c/src/main.c:74-156` exactly,
/// including the default-arm fallback `"PAR100"`.
pub fn diag_code(code: u32) -> &'static str {
    match code {
        1001 => "ERR001",
        1002 => "ERR002",
        1003 => "ERR003",
        2001 => "APP001",
        2002 => "BLD002",
        2003 => "BLD003",
        3002 => "NAM002",
        3003 => "NAM003",
        3004 => "NAM004",
        3005 => "TYP001",
        3006 => "TYP002",
        3007 => "TYP003",
        3008 => "NAM004",
        3009 => "TYP005",
        3010 => "TYP009",
        3011 => "STD002",
        3012 => "STD003",
        3013 => "OWN001",
        3014 => "OWN002",
        3015 => "MEM001",
        3016 => "TYP010",
        3017 => "TYP011",
        3018 => "TYP012",
        3019 => "TYP013",
        3020 => "TYP014",
        3021 => "TYP015",
        3022 => "TYP016",
        3023 => "TYP017",
        3024 => "TYP018",
        3025 => "TYP019",
        3026 => "TYP020",
        3027 => "TYP021",
        3028 => "TYP022",
        3029 => "BOR001",
        3030 => "BOR002",
        3031 => "ABI001",
        3032 => "TYP023",
        3033 => "TYP024",
        3034 => "TYP025",
        3035 => "MET001",
        3036 => "TYP026",
        3037 => "PUB001",
        3038 => "IFC001",
        3039 => "IFC002",
        3040 => "IFC003",
        3041 => "IFC004",
        3042 => "IFC005",
        3043 => "STC001",
        3044 => "STC002",
        3045 => "STC003",
        3046 => "SHM001",
        3047 => "SHM002",
        3048 => "RCV001",
        3049 => "RCV002",
        3101 => "FLD001",
        3102 => "FLD002",
        3103 => "VAR001",
        3104 => "VAR002",
        3105 => "MAT001",
        3106 => "MAT002",
        3107 => "MAT003",
        3108 => "VAR003",
        3109 => "VAR004",
        3110 => "MAT004",
        3111 => "MAT005",
        4004 => "CGEN004",
        5001 => "WEB001",
        6001 => "TAR001",
        6002 => "TAR002",
        7001 => "IMP001",
        7002 => "IMP002",
        7003 => "IMP003",
        8001 => "CIMP001",
        8002 => "CIMP002",
        8003 => "CIMP003",
        9001 => "PKG001",
        9002 => "PKG002",
        9003 => "PKG003",
        9004 => "PKG004",
        _ => "PAR100",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_codes_resolve() {
        assert_eq!(diag_code(1001), "ERR001");
        assert_eq!(diag_code(3010), "TYP009");
        assert_eq!(diag_code(3015), "MEM001");
        assert_eq!(diag_code(8003), "CIMP003");
        assert_eq!(diag_code(9004), "PKG004");
    }

    #[test]
    fn duplicate_mapping_matches_c() {
        // Both 3004 and 3008 map to NAM004 in the C compiler.
        assert_eq!(diag_code(3004), "NAM004");
        assert_eq!(diag_code(3008), "NAM004");
    }

    #[test]
    fn unknown_codes_default_to_par100() {
        assert_eq!(diag_code(0), "PAR100");
        assert_eq!(diag_code(42), "PAR100");
        assert_eq!(diag_code(99_999), "PAR100");
    }

    #[test]
    fn diag_struct_carries_code_text() {
        let d = Diag {
            code: 3010,
            ..Default::default()
        };
        assert_eq!(d.code_text(), "TYP009");
    }
}
