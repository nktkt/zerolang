//! `zero` CLI entry point.
//!
//! Phase 0 scaffold: prints a placeholder and exits non-zero so any caller
//! that accidentally runs the Rust binary in place of the C one notices.

fn main() {
    eprintln!(
        "zero (Rust port) — scaffold only; the C compiler at .zero/bin/zero remains the released binary."
    );
    std::process::exit(2);
}
