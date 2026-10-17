use super::*;

#[test]
fn wide_picture_shrinks_to_the_limit() {
    assert_eq!(fit_to_side(16000, 4000, 8192), (8192, 2048));
    assert_eq!(fit_to_side(4000, 16000, 8192), (2048, 8192));
}

#[test]
fn small_picture_is_untouched() {
    assert_eq!(fit_to_side(800, 600, 8192), (800, 600));
    assert_eq!(fit_to_side(8192, 8192, 8192), (8192, 8192));
}

#[test]
fn thin_picture_keeps_at_least_one_pixel() {
    assert_eq!(fit_to_side(16384, 1, 8192), (8192, 1));
}

#[test]
fn saved_name_uses_column_row_and_extension() {
    let st = BinaryViewerState {
        size: DialogSize::Normal,
        column: "My Photo".into(),
        row: 2,
        bytes: Vec::new(),
        kind: BinaryKind::Png,
        texture: None,
        image_size: None,
        image_error: None,
        pdf: Default::default(),
        pdf_done: None,
        hex: None,
        zip: None,
        gunzip: Default::default(),
        gunzip_done: None,
        json: None,
        message: None,
    };
    assert_eq!(file_name(&st), "my_photo_row3.png");
}

#[test]
fn long_text_is_cut_on_a_character_boundary() {
    let text = "a".repeat(TEXT_MAX - 1) + "é tail";
    assert_eq!(text_head(&text).len(), TEXT_MAX - 1);
    assert_eq!(text_head("short"), "short");
}

#[test]
fn gunzip_keeps_only_what_the_window_shows() {
    use std::io::Write;
    let gz = |data: &[u8]| {
        let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        e.write_all(data).unwrap();
        e.finish().unwrap()
    };
    let text = "x".repeat(TEXT_MAX * 3);
    let v = gunzip_view(&gz(text.as_bytes()), u64::MAX).unwrap();
    assert_eq!((v.size, v.cut), (TEXT_MAX * 3, false));
    assert_eq!(v.body.unwrap().len(), TEXT_MAX);

    let v = gunzip_view(&gz(&[0u8, 1, 2, 3]), u64::MAX).unwrap();
    assert!(matches!(v.body, Err((_, ref hex)) if hex.contains("00 01 02 03")));

    // The Settings limit is the only one.
    let v = gunzip_view(&gz(text.as_bytes()), 1000).unwrap();
    assert_eq!((v.size, v.cut), (1000, true));
}
