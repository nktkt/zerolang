//! Target manifest parsing and capability lookups.
//!
//! Mirrors `native/zero-c/src/target.c` (the loader side) and parses
//! `native/zero-c/targets/targets.manifest` (10 targets covering macOS,
//! Linux glibc/musl on x86_64/aarch64, Windows MSVC, and WebAssembly
//! WASI/web).
//!
//! Phase 1 scope: data model + TOML parse + capability/alias lookups.
//! The full `zero targets --json` output schema (capabilityFacts,
//! toolchain plan, libcFacts, directBackend status) lives in
//! `native/zero-c/src/main.c` and depends on host detection plus
//! per-(arch, abi, format) backend status. That layer is deferred to
//! a follow-up sub-PR within Phase 1.

use serde::{Deserialize, Serialize};
use std::io;
use std::path::Path;

pub mod derive;
pub use derive::{
    capabilities_filtered, capability_facts, direct_backend, direct_backend_reason,
    direct_backend_status, direct_exe_emitter, direct_object_emitter, has_capability,
    host_target, is_host, libc_facts, libc_mode, requires_sysroot, sysroot_env_name,
    sysroot_status, target_report, targets_report, toolchain_plan, CapabilityFact,
    DirectBackend, LibcFacts, TargetReport, TargetsReport, ToolchainPlan, CAPABILITY_LIST,
};

/// One target entry from `targets.manifest`. Field names use serde renames
/// to match the camelCase keys in the TOML and in the C compiler's
/// `--json` output.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TargetInfo {
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub os: String,
    pub arch: String,
    pub abi: String,
    #[serde(rename = "objectFormat")]
    pub object_format: String,
    pub linker: String,
    pub libc: String,
    #[serde(rename = "libcMode")]
    pub libc_mode: String,
    #[serde(rename = "exeSuffix")]
    pub exe_suffix: String,
    #[serde(rename = "zigTarget")]
    pub zig_target: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ManifestFile {
    target: Vec<TargetInfo>,
}

/// Parse a TOML manifest string into a list of `TargetInfo` entries.
pub fn parse_manifest(toml_text: &str) -> Result<Vec<TargetInfo>, toml::de::Error> {
    let mf: ManifestFile = toml::from_str(toml_text)?;
    Ok(mf.target)
}

/// Load and parse the targets manifest from a filesystem path.
pub fn load_manifest(path: impl AsRef<Path>) -> io::Result<Vec<TargetInfo>> {
    let text = std::fs::read_to_string(path)?;
    parse_manifest(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

impl TargetInfo {
    /// Returns true if this target advertises the given capability tag
    /// (e.g. "fs", "net", "stdio") in its manifest entry.
    pub fn has_capability(&self, cap: &str) -> bool {
        self.capabilities.iter().any(|c| c == cap)
    }

    /// Returns true if this target's `name` or any of its `aliases`
    /// equals the given string. Mirrors `z_find_target` in target.c.
    pub fn matches(&self, name_or_alias: &str) -> bool {
        self.name == name_or_alias || self.aliases.iter().any(|a| a == name_or_alias)
    }
}

/// Look up a target by name or alias in a manifest slice. Returns the
/// first match, or None.
pub fn find_target<'a>(targets: &'a [TargetInfo], name_or_alias: &str) -> Option<&'a TargetInfo> {
    targets.iter().find(|t| t.matches(name_or_alias))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str =
        include_str!("../../../../../native/zero-c/targets/targets.manifest");

    fn manifest() -> Vec<TargetInfo> {
        parse_manifest(MANIFEST).expect("targets.manifest parses")
    }

    #[test]
    fn manifest_has_ten_targets() {
        assert_eq!(manifest().len(), 10);
    }

    #[test]
    fn all_ten_named_targets_present() {
        let targets = manifest();
        let names: Vec<&str> = targets.iter().map(|t| t.name.as_str()).collect();
        for expected in [
            "darwin-arm64",
            "darwin-x64",
            "linux-musl-x64",
            "linux-musl-arm64",
            "linux-x64",
            "linux-arm64",
            "win32-x64.exe",
            "win32-arm64.exe",
            "wasm32-wasi",
            "wasm32-web",
        ] {
            assert!(names.contains(&expected), "missing target: {expected}");
        }
    }

    #[test]
    fn host_darwin_arm64_full_fields() {
        let targets = manifest();
        let host = find_target(&targets, "darwin-arm64").unwrap();
        assert_eq!(host.os, "macos");
        assert_eq!(host.arch, "aarch64");
        assert_eq!(host.abi, "darwin");
        assert_eq!(host.object_format, "macho");
        assert_eq!(host.linker, "cc");
        assert_eq!(host.libc, "default");
        assert_eq!(host.libc_mode, "host-default");
        assert_eq!(host.exe_suffix, "");
        assert_eq!(host.zig_target, "aarch64-macos");
        for cap in ["memory", "stdio", "args", "env", "fs", "net", "proc", "time", "rand"] {
            assert!(host.has_capability(cap), "darwin-arm64 missing capability: {cap}");
        }
        // web is intentionally absent on the host
        assert!(!host.has_capability("web"));
    }

    #[test]
    fn windows_targets_have_exe_suffix_and_coff_format() {
        let targets = manifest();
        for name in ["win32-x64.exe", "win32-arm64.exe"] {
            let t = find_target(&targets, name).unwrap();
            assert_eq!(t.exe_suffix, ".exe");
            assert_eq!(t.object_format, "coff");
            assert_eq!(t.abi, "msvc");
        }
    }

    #[test]
    fn wasm_targets_have_wasm_format() {
        let targets = manifest();
        let wasms: Vec<&TargetInfo> = targets.iter().filter(|t| t.arch == "wasm32").collect();
        assert_eq!(wasms.len(), 2);
        for t in wasms {
            assert_eq!(t.object_format, "wasm");
        }
    }

    #[test]
    fn alias_lookup_returns_canonical_name() {
        let targets = manifest();
        assert_eq!(find_target(&targets, "aarch64-macos").unwrap().name, "darwin-arm64");
        assert_eq!(
            find_target(&targets, "x86_64-linux-musl").unwrap().name,
            "linux-musl-x64"
        );
        assert_eq!(
            find_target(&targets, "aarch64-windows-msvc").unwrap().name,
            "win32-arm64.exe"
        );
    }

    #[test]
    fn unknown_target_returns_none() {
        let targets = manifest();
        assert!(find_target(&targets, "totally-fake-target").is_none());
    }

    #[test]
    fn linux_glibc_targets_require_sysroot() {
        // The C compiler treats libcMode="sysroot" as requiring an external
        // sysroot. This test pins the manifest values that drive that logic.
        let targets = manifest();
        for name in ["linux-x64", "linux-arm64"] {
            let t = find_target(&targets, name).unwrap();
            assert_eq!(t.libc, "gnu");
            assert_eq!(t.libc_mode, "sysroot");
        }
    }
}
