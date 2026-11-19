use super::*;

fn font() -> FontSettings<'static> {
    FontSettings {
        size: 14.0,
        body: BodyFont::Proportional,
        custom_path: None,
    }
}

/// Runs one frame that lays out text, which is when epaint builds the fonts
/// (and used to panic on a custom face it could not parse).
fn draw_a_frame(ctx: &egui::Context) {
    let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
        ui.label("Octa");
    });
    out.textures_delta.clear();
}

/// A custom font path naming a file that is no font (a text file, a cut-off
/// download) used to panic inside epaint on the first frame, and the path is
/// saved, so Octa crashed on every start. Now the built-in face is used.
#[test]
fn a_custom_font_that_is_no_font_falls_back_to_the_built_in_one() {
    let dir = tempfile::tempdir().unwrap();
    let text = dir.path().join("notes.ttf");
    std::fs::write(&text, "version https://git-lfs.github.com/spec/v1\n").unwrap();
    let cut = dir.path().join("cut.ttf");
    std::fs::write(&cut, &crate::fonts::ROBOTO_MEDIUM[..8]).unwrap();
    for path in [&text, &cut] {
        let path = path.to_str().unwrap();
        assert!(readable_custom_font(Some(path)).is_none(), "{path}");
        let ctx = egui::Context::default();
        apply_theme(
            &ctx,
            ThemeMode::Dark,
            FontSettings {
                custom_path: Some(path),
                ..font()
            },
        );
        draw_a_frame(&ctx);
        assert_eq!(
            ctx.global_style().text_styles[&egui::TextStyle::Body].family,
            FontFamily::Proportional
        );
    }
}

/// A real font file is still taken as the custom face.
#[test]
fn a_real_custom_font_becomes_the_body_face() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("medium.ttf");
    std::fs::write(&file, &*crate::fonts::ROBOTO_MEDIUM).unwrap();
    let path = file.to_str().unwrap();
    assert!(readable_custom_font(Some(path)).is_some());
    assert!(readable_custom_font(Some("")).is_none());
    assert!(readable_custom_font(None).is_none());
    let ctx = egui::Context::default();
    apply_theme(
        &ctx,
        ThemeMode::Dark,
        FontSettings {
            custom_path: Some(path),
            ..font()
        },
    );
    draw_a_frame(&ctx);
    assert_eq!(
        ctx.global_style().text_styles[&egui::TextStyle::Body].family,
        FontFamily::Name(Arc::from("custom"))
    );
}

/// Replays the Windows startup order on any platform: `apply_theme` runs from
/// the eframe app creator, and only afterwards does eframe push the OS theme
/// into egui's options. A light-mode Windows reports `Some(Theme::Light)`
/// there, which used to swap the whole app onto egui's stock light style while
/// Octa's own palette kept painting the toolbar dark.
#[test]
fn os_light_theme_does_not_override_a_dark_octa_theme() {
    let ctx = egui::Context::default();
    apply_theme(&ctx, ThemeMode::Dark, font());

    ctx.options_mut(|o| {
        o.begin_pass(&egui::RawInput {
            system_theme: Some(egui::Theme::Light),
            ..Default::default()
        })
    });

    assert_eq!(
        ctx.global_style().visuals.panel_fill,
        ThemeColors::for_mode(ThemeMode::Dark).bg_primary
    );
}

/// The mirror case: a light Octa theme on a dark-mode OS.
#[test]
fn os_dark_theme_does_not_override_a_light_octa_theme() {
    let ctx = egui::Context::default();
    apply_theme(&ctx, ThemeMode::Light, font());

    ctx.options_mut(|o| {
        o.begin_pass(&egui::RawInput {
            system_theme: Some(egui::Theme::Dark),
            ..Default::default()
        })
    });

    assert_eq!(
        ctx.global_style().visuals.panel_fill,
        ThemeColors::for_mode(ThemeMode::Light).bg_primary
    );
}

/// Every listed theme applies, and paints the panel in its own colour.
#[test]
fn every_listed_theme_applies_its_own_panel_colour() {
    for &mode in ThemeMode::ALL {
        let ctx = egui::Context::default();
        apply_theme(&ctx, mode, font());
        assert_eq!(
            ctx.global_style().visuals.panel_fill,
            ThemeColors::for_mode(mode).bg_primary,
            "{}",
            mode.label()
        );
    }
}

/// The point of Deep Sea Contrast: a dialog does not share the panel's
/// colour, while plain Deep Sea keeps its look.
#[test]
fn deep_sea_contrast_raises_windows_off_the_panel() {
    let visuals = |mode| {
        let ctx = egui::Context::default();
        apply_theme(&ctx, mode, font());
        ctx.global_style().visuals.clone()
    };
    let plain = visuals(ThemeMode::DeepSea);
    let contrast = visuals(ThemeMode::DeepSeaContrast);
    assert_eq!(plain.window_fill, plain.panel_fill);
    assert_ne!(contrast.window_fill, contrast.panel_fill);
    assert_eq!(contrast.panel_fill, plain.panel_fill);
}

/// Arrows, operators, circled numbers and CJK draw in every face a Markdown file is shown in, also in
/// bold text and headings. Roboto has no left or right arrow, so `->` written
/// as one character came out as an empty box (user, 2026-11-13), and the bold
/// face had no fallback at all.
#[test]
fn arrows_and_cjk_draw_in_body_bold_and_monospace_text() {
    use ab_glyph::Font as _;
    // Asked of the font files themselves: egui's `has_glyph` answers no for
    // every glyph of the face that also holds its replacement character.
    let defs = font_definitions(None);
    for family in [
        FontFamily::Proportional,
        FontFamily::Monospace,
        FontFamily::Name(Arc::from("bold")),
    ] {
        let faces: Vec<_> = defs.families[&family]
            .iter()
            .map(|name| ab_glyph::FontRef::try_from_slice(&defs.font_data[name].font).unwrap())
            .collect();
        // Common arrows and a tick, CJK, then the ones only the two symbol
        // cuts hold: return arrow, long double arrow, asterisk operator,
        // proportional to, circled one, black circled one.
        let chars = "\u{2192}\u{2190}\u{2191}\u{2193}\u{21d2}\u{2713}\u{4e2d}\
                     \u{21b5}\u{27f9}\u{2217}\u{221d}\u{2460}\u{2776}";
        for c in chars.chars() {
            assert!(
                faces.iter().any(|face| face.glyph_id(c).0 != 0),
                "{family:?} cannot draw U+{:04X}",
                c as u32
            );
        }
    }
}
