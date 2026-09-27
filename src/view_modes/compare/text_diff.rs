//! Text-diff renderer for the Compare view: a side-by-side, line-by-line
//! diff of `tab.raw_content` (left) vs `tab.compare_right_raw` (right).
//!
//! Uses `similar` for the diff computation. Each line is annotated with one
//! of three states - Equal, Insert, Delete (Replace is split into Insert +
//! Delete by the line-diff algorithm) - and painted with a small marker
//! column and a background tint so the visual reads like a `git diff`.
//!
//! The left pane is the working file and is a real editor: what is typed
//! there lands in `tab.raw_content` and saves like any other edit. The right
//! pane is a committed revision or a second file, so it is selectable and
//! copyable but never writable.

use eframe::egui;
use egui::Color32;
use similar::{ChangeTag, TextDiff};

use crate::app::state::TabState;
use crate::ui;
use ui::theme::ThemeMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RowKind {
    Equal,
    Add,   // shown green `+`
    Del,   // shown red `-`
    Blank, // pad to keep the two panes aligned
}

pub(crate) type DiffRow = (&'static str, String, RowKind);

/// Pair a flat change stream into aligned left/right rows. `similar` emits a
/// modified line as a Delete run immediately followed by an Insert run; we
/// pair them row-by-row and pad the shorter side so equal lines never drift.
/// `changes` is (tag, text) with tag in {'e','d','i'}.
pub(crate) fn pair_changes(changes: &[(char, String)]) -> (Vec<DiffRow>, Vec<DiffRow>) {
    let mut left: Vec<DiffRow> = Vec::new();
    let mut right: Vec<DiffRow> = Vec::new();
    let mut dels: Vec<String> = Vec::new();
    let mut ins: Vec<String> = Vec::new();

    fn flush(
        left: &mut Vec<DiffRow>,
        right: &mut Vec<DiffRow>,
        dels: &mut Vec<String>,
        ins: &mut Vec<String>,
    ) {
        let n = dels.len().max(ins.len());
        for i in 0..n {
            match dels.get(i) {
                Some(t) => left.push(("+", t.clone(), RowKind::Del)),
                None => left.push(("", String::new(), RowKind::Blank)),
            }
            match ins.get(i) {
                Some(t) => right.push(("-", t.clone(), RowKind::Add)),
                None => right.push(("", String::new(), RowKind::Blank)),
            }
        }
        dels.clear();
        ins.clear();
    }

    for (tag, text) in changes {
        match tag {
            'd' => {
                // similar always emits a Delete run before its paired Insert
                // run, so deletes never need pre-flushing for the normal case.
                // The guard handles the reverse order (inserts already buffered)
                // as a safety flush so a stray run can't mis-pair.
                if !ins.is_empty() {
                    flush(&mut left, &mut right, &mut dels, &mut ins);
                }
                dels.push(text.clone());
            }
            'i' => ins.push(text.clone()),
            _ => {
                flush(&mut left, &mut right, &mut dels, &mut ins);
                left.push((" ", text.clone(), RowKind::Equal));
                right.push((" ", text.clone(), RowKind::Equal));
            }
        }
    }
    flush(&mut left, &mut right, &mut dels, &mut ins);
    (left, right)
}

/// Render a side-by-side text diff into the available space. The two columns
/// share a vertical scroll so left and right line up while reading.
pub fn render(
    ui: &mut egui::Ui,
    tab: &mut TabState,
    theme_mode: ThemeMode,
    _syntax_highlight_max_bytes: usize,
    readonly: bool,
    tab_size: usize,
) {
    let colors = ui::theme::ThemeColors::for_mode(theme_mode);

    // The left pane writes back into `raw_content`, so there has to be one.
    let editable = !readonly && tab.raw_content.is_some();
    // The diff strips every line terminator, so the file's last one has to be
    // remembered separately or a save would quietly drop it.
    let trailing_newline = tab
        .raw_content
        .as_deref()
        .is_some_and(|c| c.ends_with('\n'));

    // Scoped so nothing borrows `tab` once the rows are built: the write-back
    // at the end of this function needs `tab` mutably.
    let (left_kind_rows, right_kind_rows) = {
        let left_text = tab.raw_content.as_deref().unwrap_or("");
        let right_text = tab.compare_right_raw.as_deref().unwrap_or("");

        // similar's `TextDiff` over lines is O(n²) in worst-case but ships with
        // a `timeout` knob. We cap the work at 500ms so a pathological pair
        // doesn't hang the UI thread.
        let diff = TextDiff::configure()
            .timeout(std::time::Duration::from_millis(500))
            .diff_lines(left_text, right_text);

        // Map similar's tags to our compact (tag, text) form, then pair.
        let changes: Vec<(char, String)> = diff
            .iter_all_changes()
            .map(|c| {
                let tag = match c.tag() {
                    ChangeTag::Equal => 'e',
                    ChangeTag::Delete => 'd', // left-only (current) -> +
                    ChangeTag::Insert => 'i', // right-only (compared) -> -
                };
                (tag, strip_trailing_newline(&c.to_string()))
            })
            .collect();
        pair_changes(&changes)
    };

    let add_bg = if theme_mode.is_dark() {
        Color32::from_rgb(20, 60, 32)
    } else {
        Color32::from_rgb(220, 248, 220)
    };
    let del_bg = if theme_mode.is_dark() {
        Color32::from_rgb(75, 28, 32)
    } else {
        Color32::from_rgb(255, 220, 220)
    };

    // Plain text for the whole-side copy actions. Left = current file, Right
    // = compared file.
    let join_side = |rows: &[DiffRow]| -> String {
        rows.iter()
            .filter(|r| r.2 != RowKind::Blank)
            .map(|r| r.1.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    };
    let left_text_full = left_kind_rows
        .iter()
        .map(|r| r.1.clone())
        .collect::<Vec<_>>();
    let right_text_full = right_kind_rows
        .iter()
        .map(|r| r.1.clone())
        .collect::<Vec<_>>();
    let full_copies = FullCopies {
        left: join_side(&left_kind_rows),
        right: join_side(&right_kind_rows),
        unified: left_kind_rows
            .iter()
            .zip(right_kind_rows.iter())
            .flat_map(|(l, r)| {
                let mut out = Vec::new();
                match (l.2, r.2) {
                    (RowKind::Equal, _) => out.push(format!(" {}", l.1)),
                    _ => {
                        if l.2 == RowKind::Del {
                            out.push(format!("+{}", l.1));
                        }
                        if r.2 == RowKind::Add {
                            out.push(format!("-{}", r.1));
                        }
                    }
                }
                out
            })
            .collect::<Vec<_>>()
            .join("\n"),
    };

    // Per-pane data: gutter text (line no + marker), content text (one line per
    // row, blanks = empty lines so the two panes stay row-aligned), and the
    // RowKind per line for colouring.
    let left_kinds: Vec<RowKind> = left_kind_rows.iter().map(|r| r.2).collect();
    let right_kinds: Vec<RowKind> = right_kind_rows.iter().map(|r| r.2).collect();
    let gutter = |rows: &[DiffRow]| -> String {
        rows.iter()
            .enumerate()
            .map(|(i, r)| {
                if r.2 == RowKind::Blank {
                    String::new()
                } else {
                    format!("{:>5} {}", i + 1, r.0)
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let left_gutter = gutter(&left_kind_rows);
    let right_gutter = gutter(&right_kind_rows);
    let mut left_content = left_text_full.join("\n");
    let right_content = right_text_full.join("\n");
    // The padded pane as it was handed to the editor, plus which of its rows
    // are padding: together they map an edit back onto the file.
    let left_before = left_content.clone();
    let left_padding: Vec<bool> = left_kinds.iter().map(|k| *k == RowKind::Blank).collect();

    let mono = egui::FontId::new(12.0, egui::FontFamily::Monospace);
    let mut left_changed = false;

    egui::ScrollArea::vertical()
        .id_salt("compare_text_diff_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let total_w = ui.available_width();
            let pane_w = ((total_w - 8.0) / 2.0).max(160.0);
            ui.horizontal_top(|ui| {
                // A read-only left pane hands the editor a `&str`, which egui
                // treats as immutable: same selection and copy, no writes.
                let mut left_locked: &str = &left_before;
                let left_buf: &mut dyn egui::TextBuffer = if editable {
                    &mut left_content
                } else {
                    &mut left_locked
                };
                left_changed = render_pane(
                    ui,
                    Pane {
                        tag: "left",
                        gutter_text: &left_gutter,
                        content: left_buf,
                        kinds: &left_kinds,
                        changed_bg: add_bg,
                        tab_size: editable.then_some(tab_size),
                    },
                    pane_w,
                    &mono,
                    &colors,
                    &full_copies,
                );
                ui.add_space(8.0);
                // The right side is a committed revision or a second file:
                // there is nothing to write back to, so it is never writable.
                let mut right_locked: &str = &right_content;
                render_pane(
                    ui,
                    Pane {
                        tag: "right",
                        gutter_text: &right_gutter,
                        content: &mut right_locked,
                        kinds: &right_kinds,
                        changed_bg: del_bg,
                        tab_size: None,
                    },
                    pane_w,
                    &mono,
                    &colors,
                    &full_copies,
                );
            });
        });

    // `editable` as well as `changed`: egui reports a change for any mutating
    // key event, even when the buffer it was handed refuses to mutate, so a
    // read-only tab would otherwise be marked dirty by a stray keystroke.
    if editable && left_changed {
        tab.raw_content = Some(strip_padding(
            &left_before,
            &left_padding,
            &left_content,
            trailing_newline,
        ));
        tab.raw_content_modified = true;
    }
}

/// Plain-text payloads for the whole-side copy actions in the context menu.
struct FullCopies {
    left: String,
    right: String,
    unified: String,
}

/// Marker/line-number gutter width. Room for a 5-digit line number, a space
/// and the one-char change marker.
const GUTTER_W: f32 = 54.0;

/// Render one side of the diff: a non-interactive line-number/marker gutter
/// plus a selectable, colourised content editor. The content editor lets the
/// user mark words/lines with the mouse and copy them (Ctrl+C, native) or via
/// the right-click "Copy selection" (served from a per-pane stash, since a
/// right-click collapses the live selection).
/// One side of the diff: everything that differs between the left and right
/// panes.
///
/// `tag`, `gutter_text` and `content_text` were three adjacent `&str` in the
/// old positional list. Swapping the last two compiled and rendered the line
/// numbers as the file body; naming them removes that.
struct Pane<'a> {
    tag: &'a str,
    gutter_text: &'a str,
    /// The pane's text. A `&mut String` is a real editor; a `&mut &str` is
    /// egui's immutable buffer, which still selects, scrolls and copies but
    /// swallows nothing: keystrokes simply do not apply.
    content: &'a mut dyn egui::TextBuffer,
    kinds: &'a [RowKind],
    changed_bg: Color32,
    /// Tab width for an editable pane, `None` for a read-only one. Drives
    /// both the Tab handling and whether Tab is caught at all.
    tab_size: Option<usize>,
}

/// Returns true when the user changed an editable pane this frame.
fn render_pane(
    ui: &mut egui::Ui,
    pane: Pane<'_>,
    pane_w: f32,
    mono: &egui::FontId,
    colors: &ui::theme::ThemeColors,
    copies: &FullCopies,
) -> bool {
    let Pane {
        tag,
        gutter_text,
        content,
        kinds,
        changed_bg,
        tab_size,
    } = pane;
    let mut changed = false;
    ui.scope(|ui| {
        ui.set_width(pane_w);
        ui.horizontal_top(|ui| {
            // Line-number + marker gutter. Non-interactive so it can't be
            // selected/copied (keeps copied text free of line numbers). A
            // layouter tints the change marker green/red.
            let gutter_kinds: Vec<RowKind> = kinds.to_vec();
            let gutter_font = mono.clone();
            let muted = colors.text_muted;
            let mut gutter_layouter = move |ui: &egui::Ui, text: &dyn egui::TextBuffer, _w: f32| {
                let mut job = egui::text::LayoutJob::default();
                job.wrap.max_width = f32::INFINITY;
                for (i, line) in text.as_str().split('\n').enumerate() {
                    if i > 0 {
                        job.append(
                            "\n",
                            0.0,
                            egui::text::TextFormat::simple(gutter_font.clone(), muted),
                        );
                    }
                    // Colour the trailing marker char; the number stays muted.
                    let marker_color = match gutter_kinds.get(i) {
                        Some(RowKind::Del) => Color32::from_rgb(60, 160, 80),
                        Some(RowKind::Add) => Color32::from_rgb(200, 70, 70),
                        _ => muted,
                    };
                    if let Some((num, marker)) = line.rsplit_once(' ') {
                        job.append(
                            num,
                            0.0,
                            egui::text::TextFormat::simple(gutter_font.clone(), muted),
                        );
                        job.append(
                            " ",
                            0.0,
                            egui::text::TextFormat::simple(gutter_font.clone(), muted),
                        );
                        job.append(
                            marker,
                            0.0,
                            egui::text::TextFormat::simple(gutter_font.clone(), marker_color),
                        );
                    } else {
                        job.append(
                            line,
                            0.0,
                            egui::text::TextFormat::simple(gutter_font.clone(), muted),
                        );
                    }
                }
                ui.fonts_mut(|f| f.layout_job(job))
            };
            ui.add(
                egui::TextEdit::multiline(&mut gutter_text.to_owned())
                    .font(mono.clone())
                    .interactive(false)
                    .desired_width(GUTTER_W)
                    .frame(egui::Frame::NONE)
                    .layouter(&mut gutter_layouter),
            );

            // Content: selectable editor with a diff-tinting layouter. No wrap,
            // so long lines scroll horizontally and line numbers stay aligned.
            let content_kinds: Vec<RowKind> = kinds.to_vec();
            let content_font = mono.clone();
            let text_color = colors.text_primary;
            let mut content_layouter =
                move |ui: &egui::Ui, text: &dyn egui::TextBuffer, _w: f32| {
                    let mut job = egui::text::LayoutJob::default();
                    job.wrap.max_width = f32::INFINITY;
                    for (i, line) in text.as_str().split('\n').enumerate() {
                        if i > 0 {
                            job.append(
                                "\n",
                                0.0,
                                egui::text::TextFormat::simple(content_font.clone(), text_color),
                            );
                        }
                        // Left pane rows are Del, right pane rows are Add; either
                        // way a changed row gets this pane's tint behind its text.
                        let bg = match content_kinds.get(i) {
                            Some(RowKind::Add | RowKind::Del) => changed_bg,
                            _ => Color32::TRANSPARENT,
                        };
                        job.append(
                            line,
                            0.0,
                            egui::text::TextFormat {
                                font_id: content_font.clone(),
                                color: text_color,
                                background: bg,
                                ..Default::default()
                            },
                        );
                    }
                    ui.fonts_mut(|f| f.layout_job(job))
                };

            egui::ScrollArea::horizontal()
                .id_salt(("compare_text_diff_h", tag))
                .max_width((pane_w - GUTTER_W).max(40.0))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    // `lock_focus` only on the editable side: catching Tab in a
                    // pane that cannot take it would just trap the focus.
                    let mut out = egui::TextEdit::multiline(content)
                        .font(mono.clone())
                        .desired_width(f32::INFINITY)
                        .frame(egui::Frame::NONE)
                        .lock_focus(tab_size.is_some())
                        .layouter(&mut content_layouter)
                        .show(ui);

                    // Same Tab handling as the Raw and Markdown editors.
                    let expanded = match tab_size {
                        Some(n) => crate::view_modes::text_ops::expand_tabs_to_spaces(
                            ui.ctx(),
                            content,
                            &mut out,
                            n,
                        ),
                        None => false,
                    };
                    changed = expanded || out.response.changed();

                    // Drag a selection past the edge of the pane and the view
                    // follows, so it is not capped at the lines on screen.
                    crate::view_modes::text_ops::autoscroll_while_selecting(ui, &out.response);

                    // Read back after the edit so a selection made on the same
                    // frame indexes the text the user is actually looking at.
                    let content_text = content.as_str();

                    // Stash the current non-empty selection so the right-click
                    // menu can copy it (a right-click collapses the live one).
                    let stash_id = ui.make_persistent_id(("compare_diff_sel", tag));
                    if let Some(range) = out.cursor_range {
                        let (p, s) = (range.primary.index.0, range.secondary.index.0);
                        let (a, b) = (p.min(s), p.max(s));
                        if b > a {
                            let sel: String = content_text.chars().skip(a).take(b - a).collect();
                            ui.ctx().data_mut(|d| d.insert_temp(stash_id, sel));
                        }
                    }

                    out.response.context_menu(|ui| {
                        let stashed: Option<String> = ui.ctx().data(|d| d.get_temp(stash_id));
                        if let Some(sel) = stashed.filter(|s| !s.is_empty()) {
                            if ui.button(octa::i18n::t("compare.copy_selection")).clicked() {
                                ui.ctx().copy_text(sel);
                                ui.close();
                            }
                            ui.separator();
                        }
                        if ui.button(octa::i18n::t("compare.copy_left")).clicked() {
                            ui.ctx().copy_text(copies.left.clone());
                            ui.close();
                        }
                        if ui.button(octa::i18n::t("compare.copy_right")).clicked() {
                            ui.ctx().copy_text(copies.right.clone());
                            ui.close();
                        }
                        if ui.button(octa::i18n::t("compare.copy_unified")).clicked() {
                            ui.ctx().copy_text(copies.unified.clone());
                            ui.close();
                        }
                    });
                });
        });
    });
    changed
}

/// Map the edited left pane back onto the file text it was built from.
///
/// The pane the user types in carries blank rows that exist only to keep the
/// two sides aligned; they are not in the file. `was_padding` marks those rows
/// in `old_padded`, the text as it was handed to the editor. Diffing the two
/// versions rather than counting lines is what makes this survive Enter,
/// backspace and paste: a padding row is dropped only where the diff still
/// recognises it as that untouched row, and anything the user put there is a
/// real line of the file.
///
/// `trailing_newline` restores the file's final line terminator, which the
/// line diff strips and the pane never shows.
///
/// ponytail: when the user adds a blank line right beside a padding row the
/// two are indistinguishable and the diff picks one arbitrarily. The line
/// count comes out right either way, which is all the file sees.
fn strip_padding(
    old_padded: &str,
    was_padding: &[bool],
    new_padded: &str,
    trailing_newline: bool,
) -> String {
    let old_lines: Vec<&str> = old_padded.split('\n').collect();
    let new_lines: Vec<&str> = new_padded.split('\n').collect();
    let diff = TextDiff::from_slices(&old_lines, &new_lines);
    let mut out: Vec<&str> = Vec::new();
    for change in diff.iter_all_changes() {
        match change.tag() {
            // Gone from the pane, so gone from the file.
            ChangeTag::Delete => {}
            // Survived untouched: keep it unless it was only ever padding.
            ChangeTag::Equal => {
                let is_pad = change
                    .old_index()
                    .and_then(|i| was_padding.get(i))
                    .copied()
                    .unwrap_or(false);
                if !is_pad {
                    out.push(change.value());
                }
            }
            // Typed by the user, wherever it landed.
            ChangeTag::Insert => out.push(change.value()),
        }
    }
    let mut text = out.join("\n");
    if trailing_newline && !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

fn strip_trailing_newline(s: &str) -> String {
    s.strip_suffix('\n').unwrap_or(s).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // (tag, text): 'e'=Equal, 'd'=Delete (left-only), 'i'=Insert (right-only)
    fn changes(spec: &[(char, &str)]) -> Vec<(char, String)> {
        spec.iter().map(|(t, s)| (*t, s.to_string())).collect()
    }

    #[test]
    fn modified_line_pairs_on_same_row() {
        // line2 changed: delete old, insert new -> one row, old left / new right.
        let (left, right) = pair_changes(&changes(&[
            ('e', "line1"),
            ('d', "old2"),
            ('i', "new2"),
            ('e', "line3"),
        ]));
        assert_eq!(left.len(), right.len());
        assert_eq!(left.len(), 3); // line1, changed, line3 -> no drift
        assert_eq!(left[1].2, RowKind::Del);
        assert_eq!(left[1].1, "old2");
        assert_eq!(right[1].2, RowKind::Add);
        assert_eq!(right[1].1, "new2");
        assert_eq!(left[2].1, "line3");
        assert_eq!(right[2].1, "line3");
    }

    #[test]
    fn unequal_run_lengths_pad_the_shorter_side() {
        // 1 delete, 2 inserts -> max(1,2)=2 rows; left[1] is a blank pad.
        let (left, right) = pair_changes(&changes(&[('d', "a"), ('i', "x"), ('i', "y")]));
        assert_eq!(left.len(), 2);
        assert_eq!(right.len(), 2);
        assert_eq!(left[1].2, RowKind::Blank);
        assert_eq!(right[1].1, "y");
    }
}

#[cfg(test)]
mod edit_tests {
    use super::*;

    /// The pane as the user sees it: `file` padded with a blank row wherever
    /// the right side had a line of its own. Returns (padded text, flags).
    fn pane(rows: &[Option<&str>]) -> (String, Vec<bool>) {
        let text = rows
            .iter()
            .map(|r| r.unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n");
        (text, rows.iter().map(|r| r.is_none()).collect())
    }

    #[test]
    fn an_untouched_pane_maps_back_to_the_file_it_came_from() {
        let (padded, pad) = pane(&[Some("a"), None, Some("b"), None, Some("c")]);
        assert_eq!(strip_padding(&padded, &pad, &padded, false), "a\nb\nc");
    }

    #[test]
    fn editing_a_real_line_keeps_the_edit_and_drops_the_padding() {
        let (padded, pad) = pane(&[Some("a"), None, Some("b"), Some("c")]);
        let edited = padded.replace("b", "bZ");
        assert_eq!(strip_padding(&padded, &pad, &edited, false), "a\nbZ\nc");
    }

    #[test]
    fn typing_on_a_padding_row_turns_it_into_a_real_line() {
        let (padded, pad) = pane(&[Some("a"), None, Some("b")]);
        let edited = "a\nNEW\nb";
        assert_eq!(strip_padding(&padded, &pad, edited, false), "a\nNEW\nb");
    }

    #[test]
    fn deleting_a_real_line_removes_it_from_the_file() {
        let (padded, pad) = pane(&[Some("a"), None, Some("b"), Some("c")]);
        let edited = "a\n\nc";
        assert_eq!(strip_padding(&padded, &pad, edited, false), "a\nc");
    }

    #[test]
    fn a_blank_line_the_user_adds_survives_next_to_a_padding_row() {
        // The file legitimately gains an empty line right where padding sits;
        // the count must come out right even though both lines read "".
        let (padded, pad) = pane(&[Some("a"), None, Some("b")]);
        let edited = "a\n\n\nb";
        assert_eq!(strip_padding(&padded, &pad, edited, false), "a\n\nb");
    }

    #[test]
    fn a_file_that_ended_in_a_newline_still_does() {
        // The pane never shows the final terminator, so an edit must not be
        // the thing that removes it.
        let (padded, pad) = pane(&[Some("a"), None, Some("b")]);
        let edited = padded.replace("b", "bZ");
        assert_eq!(strip_padding(&padded, &pad, &edited, true), "a\nbZ\n");
    }

    #[test]
    fn a_pane_with_no_padding_is_returned_unchanged() {
        let (padded, pad) = pane(&[Some("a"), Some("b")]);
        assert_eq!(strip_padding(&padded, &pad, "a\nbb", false), "a\nbb");
    }
}
