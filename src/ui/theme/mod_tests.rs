use super::*;

fn font() -> FontSettings<'static> {
    FontSettings {
        size: 14.0,
        body: BodyFont::Proportional,
        custom_path: None,
    }
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
