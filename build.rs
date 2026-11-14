/// The fonts `src/fonts.rs` bundles.
const FONTS: &[&str] = &[
    "Roboto-Medium.ttf",
    "Roboto-Bold.ttf",
    "JetBrainsMono-Regular.ttf",
    "NotoSansCJK-subset.otf",
    "NotoSansSymbols2-Regular.ttf",
    "NotoSansMath-subset.ttf",
    "NotoSansSymbols-subset.ttf",
];

/// Compress the translations and fonts into `$OUT_DIR/embedded/<file>.zst` for
/// `include_bytes!`. As plain files they were 20.7 MB of the binary, and packed
/// about 8 MB; `i18n` and `fonts` unpack what they need when it is first used.
fn compress_embedded_files() {
    let out = std::path::Path::new(&std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"))
        .join("embedded");
    std::fs::create_dir_all(&out).expect("creating the folder for embedded files");
    // Only a release binary's size matters, and level 19 takes seconds.
    let level = if std::env::var("PROFILE").as_deref() == Ok("release") {
        19
    } else {
        1
    };

    println!("cargo:rerun-if-changed=locales");
    let mut sources: Vec<std::path::PathBuf> = std::fs::read_dir("locales")
        .expect("reading locales/")
        .map(|entry| entry.expect("reading locales/").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
        .collect();
    for font in FONTS {
        let path = std::path::Path::new("assets").join(font);
        println!("cargo:rerun-if-changed={}", path.display());
        sources.push(path);
    }
    for source in sources {
        let raw =
            std::fs::read(&source).unwrap_or_else(|e| panic!("reading {}: {e}", source.display()));
        let packed = zstd::encode_all(raw.as_slice(), level)
            .unwrap_or_else(|e| panic!("compressing {}: {e}", source.display()));
        let name = source
            .file_name()
            .expect("a file has a name")
            .to_string_lossy();
        std::fs::write(out.join(format!("{name}.zst")), packed)
            .unwrap_or_else(|e| panic!("writing {name}.zst: {e}"));
    }
}

fn main() {
    // With a `rerun-if-changed` line anywhere, cargo reruns this only for the
    // paths named, rather than for every file of the package.
    println!("cargo:rerun-if-changed=build.rs");
    compress_embedded_files();

    // Windows gives the main thread a 1 MB stack, Linux and macOS 8 MB. In a
    // debug build clap's derive for `Cli` alone takes 1.6 MB of it
    // (`augment_args` keeps every argument's builder in its own slot), so the
    // debug binary died before `--version` printed, and every Windows test
    // that runs it with it. A release build needs under 128 KB. Reserve what
    // the other systems give, for the binary only; the GUI's main thread gets
    // the same room.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        println!("cargo:rustc-link-arg-bins=/STACK:8388608");
    }

    #[cfg(target_os = "windows")]
    {
        println!("cargo:rerun-if-changed=windows/octa.exe.manifest");
        println!("cargo:rerun-if-changed=assets/octa.ico");
        let mut res = winresource::WindowsResource::new();
        res.set_manifest_file("windows/octa.exe.manifest");
        res.set("ProductName", "Octa");
        res.set("FileDescription", "Multi-format data viewer and editor");
        res.set("LegalCopyright", "MIT License");
        if std::path::Path::new("assets/octa.ico").exists() {
            res.set_icon("assets/octa.ico");
        }
        res.compile().expect("Failed to compile Windows resources");
    }
}
