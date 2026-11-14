use super::*;

#[test]
fn every_font_unpacks_to_its_file_byte_for_byte() {
    for (font, file) in [
        (&ROBOTO_MEDIUM, "Roboto-Medium.ttf"),
        (&ROBOTO_BOLD, "Roboto-Bold.ttf"),
        (&JETBRAINS_MONO, "JetBrainsMono-Regular.ttf"),
        (&NOTO_CJK_SUBSET, "NotoSansCJK-subset.otf"),
        (&NOTO_SYMBOLS2, "NotoSansSymbols2-Regular.ttf"),
        (&NOTO_MATH_SUBSET, "NotoSansMath-subset.ttf"),
        (&NOTO_SYMBOLS_SUBSET, "NotoSansSymbols-subset.ttf"),
    ] {
        let on_disk = std::fs::read(format!("assets/{file}")).unwrap();
        // Not `assert_eq!`: a failure would print megabytes.
        assert!(font.as_slice() == on_disk.as_slice(), "{file}");
    }
}
