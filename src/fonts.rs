//! The fonts bundled into the binary, shared by the GUI (`ui::theme`) and the
//! PDF exports (`data::pdf_export`), so a PDF looks like the app on every
//! machine instead of like whatever the system maps "sans-serif" to. They sit in
//! the binary compressed (`build.rs`) and unpack on first use.

use std::sync::LazyLock;

/// A font from `$OUT_DIR/embedded`, unpacked when first read.
macro_rules! font {
    ($file:literal) => {
        LazyLock::new(|| {
            let packed: &[u8] =
                include_bytes!(concat!(env!("OUT_DIR"), "/embedded/", $file, ".zst"));
            zstd::decode_all(packed).expect("a bundled font unpacks")
        })
    };
}

/// Roboto Medium, the app's body face. Apache-2.0 (`licenses/Apache-2.0.txt`).
pub static ROBOTO_MEDIUM: LazyLock<Vec<u8>> = font!("Roboto-Medium.ttf");
/// Roboto Bold. Apache-2.0.
pub static ROBOTO_BOLD: LazyLock<Vec<u8>> = font!("Roboto-Bold.ttf");
/// JetBrains Mono Regular, the SQL editor's face. OFL-1.1 (`licenses/OFL-1.1.txt`).
pub static JETBRAINS_MONO: LazyLock<Vec<u8>> = font!("JetBrainsMono-Regular.ttf");
/// A `pyftsubset` cut of Noto Sans CJK SC (common CJK, kana, hangul). OFL-1.1.
pub static NOTO_CJK_SUBSET: LazyLock<Vec<u8>> = font!("NotoSansCJK-subset.otf");
/// Noto Sans Symbols 2, for miscellaneous symbols. OFL-1.1.
pub static NOTO_SYMBOLS2: LazyLock<Vec<u8>> = font!("NotoSansSymbols2-Regular.ttf");
/// The arrows, operators and technical signs of Noto Sans Math that no other
/// bundled face draws (`scripts/build-symbol-fonts.py`). OFL-1.1.
pub static NOTO_MATH_SUBSET: LazyLock<Vec<u8>> = font!("NotoSansMath-subset.ttf");
/// What is then still missing, from Noto Sans Symbols: circled numbers and
/// more symbols. Same script. OFL-1.1.
pub static NOTO_SYMBOLS_SUBSET: LazyLock<Vec<u8>> = font!("NotoSansSymbols-subset.ttf");

#[cfg(test)]
#[path = "fonts_tests.rs"]
mod tests;
