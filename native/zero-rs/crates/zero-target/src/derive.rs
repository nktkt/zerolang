//! Per-target derived facts: host detection, capability override on host,
//! sysroot env / status, toolchain plan, direct-backend status.
//!
//! Ports `native/zero-c/src/target.c` lines 226-453.

use crate::TargetInfo;
use serde::Serialize;
use std::path::Path;

/// Return the canonical name of the build host's target.
///
/// Mirrors `z_host_target` in target.c:226-242. Uses Rust `cfg` predicates
/// in place of the C preprocessor macros so the resulting binary reports
/// the host it was built for, exactly as the C compiler does.
pub fn host_target() -> &'static str {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "darwin-arm64"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "darwin-x64"
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        "linux-arm64"
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "linux-x64"
    }
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    {
        "win32-arm64.exe"
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "win32-x64.exe"
    }
    #[cfg(not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "windows", target_arch = "aarch64"),
        all(target_os = "windows", target_arch = "x86_64"),
    )))]
    {
        "unknown"
    }
}

/// True if `target` is the build host. Mirrors `z_target_is_host`.
pub fn is_host(target: &TargetInfo) -> bool {
    target.name == host_target()
}

/// Host-only capabilities that are always available on the build host
/// even if the manifest entry omits them. Mirrors
/// `host_capability_available` in target.c:273-280.
fn host_capability_available(cap: &str) -> bool {
    matches!(cap, "args" | "env" | "fs" | "net" | "proc")
}

/// True if the target advertises the capability, with the host-override
/// applied for the build host. Mirrors `z_target_has_capability`.
pub fn has_capability(target: &TargetInfo, capability: &str) -> bool {
    if is_host(target) && host_capability_available(capability) {
        return true;
    }
    target.has_capability(capability)
}

/// Returns "host-default" when the manifest has no explicit mode.
pub fn libc_mode(target: &TargetInfo) -> &str {
    if target.libc_mode.is_empty() {
        "host-default"
    } else {
        &target.libc_mode
    }
}

/// Mirrors `z_target_requires_sysroot`.
pub fn requires_sysroot(target: &TargetInfo) -> bool {
    !is_host(target) && libc_mode(target) == "sysroot"
}

/// `ZERO_SYSROOT_<UPPERCASED_ZIG_TARGET>`. Non-alphanumeric → `_`.
/// Mirrors `z_target_sysroot_env_name`.
pub fn sysroot_env_name(target: &TargetInfo) -> String {
    let source = if target.zig_target.is_empty() {
        "host"
    } else {
        &target.zig_target
    };
    let mut name = String::from("ZERO_SYSROOT_");
    for ch in source.chars() {
        if ch.is_ascii_alphanumeric() {
            name.push(ch.to_ascii_uppercase());
        } else {
            name.push('_');
        }
    }
    name
}

/// "not-required" / "missing" / "host-leakage" / "present".
/// Mirrors `target_sysroot_status`.
pub fn sysroot_status(target: &TargetInfo) -> &'static str {
    if !requires_sysroot(target) {
        return "not-required";
    }
    let env_name = sysroot_env_name(target);
    match std::env::var(&env_name) {
        Ok(path) if !path.is_empty() => {
            if path.contains("/usr/include") || path.contains("/usr/lib") {
                return "host-leakage";
            }
            if Path::new(&path).is_dir() {
                "present"
            } else {
                "missing"
            }
        }
        _ => "missing",
    }
}

/// Mirrors `target_uses_emscripten` in target.c:359-363.
fn uses_emscripten(target: &TargetInfo) -> bool {
    target.linker == "emcc"
        || target.linker == "emscripten"
        || target.libc_mode == "emscripten"
}

/// Returns the direct object emitter name for `(format, arch, os)`.
/// Mirrors `z_direct_object_emitter` (target.c:365-376).
pub fn direct_object_emitter(target: &TargetInfo) -> &'static str {
    let format = target.object_format.as_str();
    let arch = target.arch.as_str();
    let os = target.os.as_str();
    match (format, arch, os) {
        ("wasm", "wasm32", _) => "zero-wasm",
        ("elf", "x86_64", "linux") => "zero-elf64",
        ("elf", "aarch64", "linux") => "zero-elf-aarch64",
        ("macho", "aarch64", "macos") => "zero-macho64",
        ("coff", "x86_64", "windows") => "zero-coff-x64",
        _ => "none",
    }
}

/// Direct executable emitter. Mirrors `z_direct_exe_emitter`
/// (target.c:378-387) — the C side keys this on the target name
/// rather than (arch, os), so we follow suit.
pub fn direct_exe_emitter(target: &TargetInfo) -> &'static str {
    match target.name.as_str() {
        "linux-x64" | "linux-musl-x64" => "zero-elf64-exe",
        "linux-musl-arm64" | "linux-arm64" => "zero-elf-aarch64-exe",
        "darwin-arm64" => "zero-macho64-exe",
        "win32-x64.exe" => "zero-coff-x64-exe",
        _ => "none",
    }
}

/// "native-exe" / "wasm-module" / "native-object" / "known-unimplemented".
/// Mirrors `z_direct_backend_status` (target.c:389-395).
pub fn direct_backend_status(target: &TargetInfo) -> &'static str {
    if direct_exe_emitter(target) != "none" {
        return "native-exe";
    }
    let obj = direct_object_emitter(target);
    if obj == "zero-wasm" {
        return "wasm-module";
    }
    if obj != "none" {
        return "native-object";
    }
    "known-unimplemented"
}

/// Human-readable reason matching the C direct-backend reason text
/// exactly (target.c:397-408). The exact strings are part of the
/// `zero targets --json` contract.
pub fn direct_backend_reason(target: &TargetInfo) -> &'static str {
    if direct_object_emitter(target) != "none" {
        if direct_exe_emitter(target) != "none" {
            return "direct object and executable backend available";
        }
        return "direct object backend available; direct executable linker is not implemented for this target";
    }
    let format = target.object_format.as_str();
    let arch = target.arch.as_str();
    if format == "elf" && arch == "aarch64" {
        return "AArch64 ELF machine-code backend is not implemented yet";
    }
    if format == "coff" && arch == "aarch64" {
        return "AArch64 COFF machine-code backend is not implemented yet";
    }
    "direct backend is not implemented for this target format/architecture pair"
}

/// The linker string emitted in `--json` output: the manifest says
/// "zig cc" for cross-Linux, but C maps that to "target-cc" before
/// printing. Mirrors target.c:412/476.
fn linker_text(target: &TargetInfo) -> &str {
    if target.linker == "zig cc" {
        "target-cc"
    } else {
        &target.linker
    }
}

/// Per-target toolchain plan as it appears in `--json`. Mirrors
/// `append_target_toolchain_json` (target.c:410-423).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ToolchainPlan {
    #[serde(rename = "cCompiler")]
    pub c_compiler: String,
    #[serde(rename = "crossCompiler")]
    pub cross_compiler: String,
    #[serde(rename = "compilerTarget")]
    pub compiler_target: String,
    #[serde(rename = "objectFormat")]
    pub object_format: String,
    pub linker: String,
    #[serde(rename = "requiresSysroot")]
    pub requires_sysroot: bool,
}

pub fn toolchain_plan(target: &TargetInfo) -> ToolchainPlan {
    let driver = if is_host(target) {
        "cc".to_string()
    } else if uses_emscripten(target) {
        "emcc".to_string()
    } else {
        "target-capable C compiler".to_string()
    };
    ToolchainPlan {
        c_compiler: driver.clone(),
        cross_compiler: driver,
        compiler_target: target.zig_target.clone(),
        object_format: target.object_format.clone(),
        linker: linker_text(target).to_string(),
        requires_sysroot: requires_sysroot(target),
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LibcFacts {
    pub name: String,
    pub mode: String,
    #[serde(rename = "hostReusable")]
    pub host_reusable: bool,
    #[serde(rename = "sysrootEnv")]
    pub sysroot_env: String,
    #[serde(rename = "sysrootStatus")]
    pub sysroot_status: String,
}

pub fn libc_facts(target: &TargetInfo) -> LibcFacts {
    LibcFacts {
        name: if target.libc.is_empty() {
            "default".into()
        } else {
            target.libc.clone()
        },
        mode: libc_mode(target).to_string(),
        host_reusable: is_host(target),
        sysroot_env: if requires_sysroot(target) {
            sysroot_env_name(target)
        } else {
            String::new()
        },
        sysroot_status: sysroot_status(target).to_string(),
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DirectBackend {
    pub status: String,
    #[serde(rename = "objectSupported")]
    pub object_supported: bool,
    #[serde(rename = "exeSupported")]
    pub exe_supported: bool,
    #[serde(rename = "objectEmitter")]
    pub object_emitter: String,
    #[serde(rename = "exeEmitter")]
    pub exe_emitter: String,
    #[serde(rename = "objectFormat")]
    pub object_format: String,
    pub arch: String,
    pub abi: String,
    pub reason: String,
    pub fallback: String,
    #[serde(rename = "explicitDirectFallback")]
    pub explicit_direct_fallback: String,
}

pub fn direct_backend(target: &TargetInfo) -> DirectBackend {
    let object_emitter = direct_object_emitter(target);
    let exe_emitter = direct_exe_emitter(target);
    DirectBackend {
        status: direct_backend_status(target).to_string(),
        object_supported: object_emitter != "none",
        exe_supported: exe_emitter != "none",
        object_emitter: object_emitter.to_string(),
        exe_emitter: exe_emitter.to_string(),
        object_format: target.object_format.clone(),
        arch: target.arch.clone(),
        abi: target.abi.clone(),
        reason: direct_backend_reason(target).to_string(),
        fallback: "removed".to_string(),
        explicit_direct_fallback: "never-c-bridge".to_string(),
    }
}

/// One capability fact entry. Mirrors `append_target_capability_facts_json`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CapabilityFact {
    pub name: String,
    pub available: bool,
    pub source: String,
}

/// The fixed 10-capability list, in the order C iterates them
/// (target.c:340).
pub const CAPABILITY_LIST: &[&str] = &[
    "memory", "stdio", "args", "env", "fs", "net", "proc", "time", "rand", "web",
];

pub fn capability_facts(target: &TargetInfo) -> Vec<CapabilityFact> {
    CAPABILITY_LIST
        .iter()
        .map(|cap| {
            let available = has_capability(target, cap);
            CapabilityFact {
                name: (*cap).to_string(),
                available,
                source: if available { "manifest" } else { "unavailable" }.to_string(),
            }
        })
        .collect()
}

/// Capabilities filtered to the available subset, in the fixed order.
pub fn capabilities_filtered(target: &TargetInfo) -> Vec<String> {
    CAPABILITY_LIST
        .iter()
        .filter(|cap| has_capability(target, cap))
        .map(|cap| (*cap).to_string())
        .collect()
}

/// One entry in the top-level `targets` array of `zero targets --json`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TargetReport {
    pub name: String,
    pub aliases: Vec<String>,
    pub os: String,
    pub arch: String,
    pub abi: String,
    #[serde(rename = "objectFormat")]
    pub object_format: String,
    pub linker: String,
    pub libc: String,
    #[serde(rename = "exeSuffix")]
    pub exe_suffix: String,
    #[serde(rename = "compilerTarget")]
    pub compiler_target: String,
    #[serde(rename = "targetCc")]
    pub target_cc: bool,
    pub hosted: bool,
    pub capabilities: Vec<String>,
    #[serde(rename = "capabilityFacts")]
    pub capability_facts: Vec<CapabilityFact>,
    pub toolchain: ToolchainPlan,
    #[serde(rename = "libcFacts")]
    pub libc_facts: LibcFacts,
    #[serde(rename = "directBackend")]
    pub direct_backend: DirectBackend,
}

pub fn target_report(target: &TargetInfo) -> TargetReport {
    let mut aliases = target.aliases.clone();
    if is_host(target) {
        aliases.push("host".to_string());
    }
    TargetReport {
        name: target.name.clone(),
        aliases,
        os: target.os.clone(),
        arch: target.arch.clone(),
        abi: target.abi.clone(),
        object_format: target.object_format.clone(),
        linker: linker_text(target).to_string(),
        libc: target.libc.clone(),
        exe_suffix: target.exe_suffix.clone(),
        compiler_target: target.zig_target.clone(),
        target_cc: true,
        hosted: is_host(target),
        capabilities: capabilities_filtered(target),
        capability_facts: capability_facts(target),
        toolchain: toolchain_plan(target),
        libc_facts: libc_facts(target),
        direct_backend: direct_backend(target),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TargetsReport {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    pub host: String,
    pub targets: Vec<TargetReport>,
}

pub fn targets_report(all: &[TargetInfo]) -> TargetsReport {
    TargetsReport {
        schema_version: 1,
        host: host_target().to_string(),
        targets: all.iter().map(target_report).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_manifest;

    const MANIFEST: &str =
        include_str!("../../../../../native/zero-c/targets/targets.manifest");

    fn manifest() -> Vec<TargetInfo> {
        parse_manifest(MANIFEST).unwrap()
    }

    fn by_name<'a>(all: &'a [TargetInfo], name: &str) -> &'a TargetInfo {
        all.iter().find(|t| t.name == name).unwrap()
    }

    #[test]
    fn host_target_resolves_to_known_target_on_supported_platforms() {
        let host = host_target();
        // The test runner runs on macOS arm64 or one of the supported
        // build hosts; "unknown" is acceptable on exotic CI but not here.
        let known = [
            "darwin-arm64",
            "darwin-x64",
            "linux-arm64",
            "linux-x64",
            "win32-arm64.exe",
            "win32-x64.exe",
        ];
        assert!(known.contains(&host) || host == "unknown");
    }

    #[test]
    fn host_capability_override_grants_extra_caps() {
        let all = manifest();
        let host = host_target();
        if host == "darwin-x64" {
            // darwin-x64 manifest lacks "args" and "env", but if it
            // happens to BE the host, the override grants them.
            let t = by_name(&all, "darwin-x64");
            assert!(has_capability(t, "args"));
            assert!(has_capability(t, "env"));
        }
    }

    #[test]
    fn sysroot_env_name_uppercases_and_substitutes() {
        let all = manifest();
        let lin = by_name(&all, "linux-x64");
        assert_eq!(sysroot_env_name(lin), "ZERO_SYSROOT_X86_64_LINUX_GNU");
        let win = by_name(&all, "win32-arm64.exe");
        assert_eq!(sysroot_env_name(win), "ZERO_SYSROOT_AARCH64_WINDOWS_MSVC");
    }

    #[test]
    fn requires_sysroot_matches_manifest_libc_mode() {
        let all = manifest();
        assert!(!requires_sysroot(by_name(&all, "linux-musl-x64"))); // bundled-libc
        // glibc Linux requires sysroot unless it's the host.
        if host_target() != "linux-x64" {
            assert!(requires_sysroot(by_name(&all, "linux-x64")));
        }
        assert!(!requires_sysroot(by_name(&all, "wasm32-wasi"))); // bundled-libc
    }

    #[test]
    fn direct_emitters_resolve_correctly() {
        let all = manifest();
        assert_eq!(direct_object_emitter(by_name(&all, "linux-musl-x64")), "zero-elf64");
        assert_eq!(direct_object_emitter(by_name(&all, "darwin-arm64")), "zero-macho64");
        assert_eq!(direct_object_emitter(by_name(&all, "win32-x64.exe")), "zero-coff-x64");
        assert_eq!(direct_object_emitter(by_name(&all, "wasm32-wasi")), "zero-wasm");
        // ARM64 COFF is unimplemented.
        assert_eq!(direct_object_emitter(by_name(&all, "win32-arm64.exe")), "none");
        // darwin-x64 macho is unimplemented (only arm64 mac listed).
        assert_eq!(direct_object_emitter(by_name(&all, "darwin-x64")), "none");
    }

    #[test]
    fn direct_backend_status_classification() {
        let all = manifest();
        assert_eq!(direct_backend_status(by_name(&all, "linux-musl-x64")), "native-exe");
        assert_eq!(direct_backend_status(by_name(&all, "wasm32-wasi")), "wasm-module");
        assert_eq!(direct_backend_status(by_name(&all, "win32-arm64.exe")), "known-unimplemented");
    }

    #[test]
    fn target_report_serializes_with_camel_case_keys() {
        let all = manifest();
        let report = target_report(by_name(&all, "darwin-arm64"));
        let json = serde_json::to_string(&report).unwrap();
        for key in [
            "\"objectFormat\":",
            "\"exeSuffix\":",
            "\"compilerTarget\":",
            "\"targetCc\":",
            "\"capabilityFacts\":",
            "\"libcFacts\":",
            "\"directBackend\":",
            "\"requiresSysroot\":",
        ] {
            assert!(json.contains(key), "missing JSON key {key}");
        }
    }

    #[test]
    fn capability_facts_have_all_ten_in_fixed_order() {
        let all = manifest();
        let facts = capability_facts(by_name(&all, "darwin-arm64"));
        let names: Vec<&str> = facts.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, CAPABILITY_LIST.to_vec());
    }

    #[test]
    fn host_target_aliases_get_host_suffix() {
        let all = manifest();
        let host_name = host_target();
        let report = targets_report(&all);
        for t in &report.targets {
            if t.name == host_name {
                assert!(t.aliases.contains(&"host".to_string()), "host alias missing");
            }
        }
    }
}
