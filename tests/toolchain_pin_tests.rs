//! CI installs the Rust version named in `rust-toolchain.toml`. A system Rust
//! (Arch's package, no rustup) ignores that file, so nothing else would notice
//! the two drifting apart until CI lints with a different Clippy.

#[test]
fn the_pinned_rust_is_the_rust_that_runs_here() {
    let pin = include_str!("../rust-toolchain.toml")
        .lines()
        .find_map(|l| l.strip_prefix("channel = "))
        .expect("rust-toolchain.toml names a channel")
        .trim()
        .trim_matches('"');
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let out = std::process::Command::new(rustc)
        .arg("--version")
        .output()
        .expect("rustc runs");
    let version = String::from_utf8_lossy(&out.stdout);
    assert!(
        version.starts_with(&format!("rustc {pin} ")),
        "rust-toolchain.toml pins Rust {pin}, but this machine runs {version}\
         Set `channel` in rust-toolchain.toml to the local version, so CI uses the same."
    );
}
