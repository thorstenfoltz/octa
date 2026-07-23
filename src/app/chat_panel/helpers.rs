//! Small rendering/formatting helpers and mention-prefix parsing for the chat
//! panel. Split out of chat_panel/mod.rs.

use eframe::egui;
use serde_json::Value;

use octa::data::DataTable;
use octa::i18n::t;

/// A clone of `table` with cell edits materialised, so tools see the user's
/// in-memory changes without the live table being mutated.
pub(crate) fn snapshot_table(table: &DataTable) -> DataTable {
    let mut clone = table.clone();
    clone.apply_edits();
    clone
}

/// A clean tab display name (no modified `*`), for addressing via `open_tab`.
pub(crate) fn tab_display_name(tab: &crate::app::state::TabState, index: usize) -> String {
    tab.table
        .source_path
        .as_ref()
        .and_then(|p| {
            std::path::Path::new(p)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| format!("Untitled {}", index + 1))
}

/// If the caret sits inside an `@`-prefixed token (no whitespace since the
/// `@`), return the byte offset of the `@` and the partial text after it.
/// `cursor_byte` must be a char boundary. Returns `None` otherwise.
pub(crate) fn current_at_prefix(text: &str, cursor_byte: usize) -> Option<(usize, String)> {
    let upto = &text[..cursor_byte.min(text.len())];
    let token_start = upto.rfind(char::is_whitespace).map(|i| i + 1).unwrap_or(0);
    let token = &upto[token_start..];
    let stripped = token.strip_prefix('@')?;
    // A second '@' means we are past a completed mention - don't re-trigger.
    if stripped.contains('@') {
        return None;
    }
    Some((token_start, stripped.to_string()))
}

/// Render one persisted message as bubbles + tool disclosure rows.
pub(crate) fn render_message(ui: &mut egui::Ui, msg: &crate::app::chat::types::Message) {
    use crate::app::chat::types::{ContentBlock, Role};
    for block in &msg.blocks {
        match block {
            ContentBlock::Text { text } => {
                if text.trim().is_empty() {
                    continue;
                }
                let (who, user) = match msg.role {
                    Role::Assistant => (t("chat.assistant"), false),
                    _ => (t("chat.you"), true),
                };
                bubble(ui, who, text, user);
            }
            ContentBlock::ToolUse { name, input, .. } => {
                egui::CollapsingHeader::new(format!("{} {name}", t("chat.tool_call")))
                    .id_salt(("octa_chat_tooluse", name.as_str(), input.to_string().len()))
                    .show(ui, |ui| {
                        copyable_text(ui, &pretty(input), true);
                    });
            }
            ContentBlock::ToolResult {
                content, is_error, ..
            } => {
                let header = if *is_error {
                    t("chat.tool_error")
                } else {
                    t("chat.tool_result")
                };
                egui::CollapsingHeader::new(header)
                    .id_salt(("octa_chat_toolresult", content.len()))
                    // Show errors expanded so the user sees what went wrong
                    // without having to open the disclosure.
                    .default_open(*is_error)
                    .show(ui, |ui| {
                        copyable_text(ui, &clip(content, 4000), true);
                    });
            }
            // Opaque provider payloads (e.g. encrypted reasoning items) are
            // transport plumbing, not conversation; nothing to render.
            ContentBlock::ProviderData { .. } => {}
        }
    }
}

/// A selectable text block with a right-click **Copy** menu (copies the whole
/// block). Used for chat bubbles + tool output so text is grabbable both by
/// selection+Ctrl+C and by right-click, regardless of the table view's own
/// clipboard shortcut.
fn copyable_text(ui: &mut egui::Ui, text: &str, monospace: bool) {
    let text = ascii_glyphs(text);
    let text = text.as_ref();
    let rich = if monospace {
        egui::RichText::new(text).monospace()
    } else {
        egui::RichText::new(text)
    };
    let resp = ui.add(egui::Label::new(rich).selectable(true).wrap());
    resp.context_menu(|ui| {
        if ui.button(t("chat.copy")).clicked() {
            ui.ctx().copy_text(text.to_string());
            ui.close();
        }
    });
}

/// egui's bundled fonts cover Latin/Greek/Cyrillic/CJK but not the arrow and
/// typographic-symbol ranges, so glyphs the model loves to emit (`->`, em
/// dash, smart quotes, ellipsis, bullet) render as empty tofu squares. Map the
/// common offenders to ASCII before display. Script text (CJK/Greek/...) is
/// left untouched so real translations still render.
///
/// ponytail: curated map, not full Unicode transliteration. Add a glyph here
/// when one shows up as tofu rather than bundling a symbol font.
fn ascii_glyphs(s: &str) -> std::borrow::Cow<'_, str> {
    if s.is_ascii() {
        return std::borrow::Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\u{2192}' | '\u{27F6}' | '\u{21D2}' => out.push_str("->"),
            '\u{2190}' | '\u{27F5}' | '\u{21D0}' => out.push_str("<-"),
            '\u{2194}' | '\u{21D4}' => out.push_str("<->"),
            '\u{2014}' | '\u{2013}' => out.push('-'),
            '\u{2026}' => out.push_str("..."),
            '\u{2022}' | '\u{00B7}' | '\u{2027}' => out.push('*'),
            '\u{201C}' | '\u{201D}' => out.push('"'),
            '\u{2018}' | '\u{2019}' => out.push('\''),
            '\u{00D7}' => out.push('x'),
            other => out.push(other),
        }
    }
    std::borrow::Cow::Owned(out)
}

/// A simple speaker-labelled text block.
pub(crate) fn bubble(ui: &mut egui::Ui, who: String, text: &str, user: bool) {
    ui.add_space(4.0);
    let color = if user {
        egui::Color32::from_rgb(0x30, 0x70, 0xc0)
    } else {
        egui::Color32::from_rgb(0x30, 0x90, 0x50)
    };
    ui.colored_label(color, who);
    copyable_text(ui, text, false);
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string())
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max).collect();
    format!("{cut}\n...[clipped]")
}

#[cfg(test)]
mod ascii_glyph_tests {
    use super::ascii_glyphs;

    #[test]
    fn maps_common_tofu_to_ascii() {
        assert_eq!(ascii_glyphs("a \u{2192} b"), "a -> b");
        assert_eq!(ascii_glyphs("x \u{2014} y \u{2026}"), "x - y ...");
        assert_eq!(ascii_glyphs("\u{201C}hi\u{201D}"), "\"hi\"");
        // Pure ASCII is borrowed unchanged; real script text is preserved.
        assert!(matches!(
            ascii_glyphs("plain"),
            std::borrow::Cow::Borrowed(_)
        ));
        assert_eq!(ascii_glyphs("\u{4f60}\u{597d}"), "\u{4f60}\u{597d}");
    }
}
