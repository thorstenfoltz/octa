//! The fonts bundled into the binary, shared by the GUI (`ui::theme`) and the
//! PDF exports (`data::pdf_export`), so a PDF looks like the app on every
//! machine instead of like whatever the system maps "sans-serif" to.

/// Roboto Medium, the app's body face. Apache-2.0 (`licenses/Apache-2.0.txt`).
pub static ROBOTO_MEDIUM: &[u8] = include_bytes!("../assets/Roboto-Medium.ttf");
/// Roboto Bold. Apache-2.0.
pub static ROBOTO_BOLD: &[u8] = include_bytes!("../assets/Roboto-Bold.ttf");
/// JetBrains Mono Regular, the SQL editor's face. OFL-1.1 (`licenses/OFL-1.1.txt`).
pub static JETBRAINS_MONO: &[u8] = include_bytes!("../assets/JetBrainsMono-Regular.ttf");
/// A `pyftsubset` cut of Noto Sans CJK SC (common CJK, kana, hangul). OFL-1.1.
pub static NOTO_CJK_SUBSET: &[u8] = include_bytes!("../assets/NotoSansCJK-subset.otf");
/// Noto Sans Symbols 2, for miscellaneous symbols. OFL-1.1.
pub static NOTO_SYMBOLS2: &[u8] = include_bytes!("../assets/NotoSansSymbols2-Regular.ttf");
