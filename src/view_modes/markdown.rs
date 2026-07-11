use crate::app::state::{NavDir, TabState};
use octa::data::MarkdownLayout;
use octa::data::search::RowMatcher;

use eframe::egui;
use egui::{Color32, RichText};

/// Translucent highlight backgrounds for Markdown search matches. Fixed rather
/// than theme-derived because the Markdown renderer works from `ui.visuals()`
/// and isn't handed a `ThemeMode`; these read acceptably on light and dark.
const MD_HL_NORMAL: Color32 = Color32::from_rgba_premultiplied(150, 130, 0, 90);
const MD_HL_ACTIVE: Color32 = Color32::from_rgba_premultiplied(170, 100, 0, 150);

/// Apply the Markdown search highlight to an already-built inline `LayoutJob`.
/// `full_text` must equal the job's concatenated text (matching byte offsets).
fn highlight_md_job(
    job: &mut egui::text::LayoutJob,
    full_text: &str,
    hl: Option<&(RowMatcher, Color32)>,
) {
    if let Some((matcher, bg)) = hl {
        let ranges = matcher.find_ranges(full_text);
        crate::ui::search_highlight::apply_highlight(job, &ranges, None, *bg, *bg);
    }
}

/// Cheap content hash so we can invalidate `tab.markdown_render_cache` only
/// when the buffer actually changes. Uses `DefaultHasher` for simplicity -
/// collisions are harmless (worst case is one stale render before the next
/// keystroke triggers another rebuild).
fn content_hash(s: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

/// Render the Markdown view with a Preview / Split / Edit segmented toggle.
/// In Split mode the left pane is a TextEdit bound to `tab.raw_content`;
/// edits are reflected in the right-pane preview every frame.
pub fn render_markdown_view(
    ui: &mut egui::Ui,
    tab: &mut TabState,
    readonly: bool,
    tab_size: usize,
) {
    let Some(content_owned) = tab.raw_content.clone() else {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new(octa::i18n::t("view.md_na"))
                    .size(16.0)
                    .color(ui.visuals().weak_text_color()),
            );
        });
        return;
    };

    // Layout toggle bar (Preview / Split / Edit). The icons are picked from
    // NotoEmoji-Regular (bundled by epaint_default_fonts) so they render
    // reliably; the size bump pulls the supplementary-plane emoji up so it
    // doesn't disappear next to the body text. Previous picks (U+21CB and
    // U+270E) lived in Unicode blocks that the bundled font set doesn't cover
    // and rendered as tofu squares.
    ui.horizontal(|ui| {
        let mut layout = tab.markdown_layout;
        // Match the toggle-button text size so the row reads as one block
        // instead of "tiny label + huge buttons".
        ui.label(RichText::new(octa::i18n::t("view.md_layout")).size(15.0));
        ui.selectable_value(
            &mut layout,
            MarkdownLayout::Preview,
            RichText::new(format!("\u{1f441} {}", octa::i18n::t("view.md_preview"))).size(15.0),
        );
        ui.selectable_value(
            &mut layout,
            MarkdownLayout::Split,
            RichText::new(format!("\u{1f500} {}", octa::i18n::t("view.md_split"))).size(15.0),
        );
        ui.selectable_value(
            &mut layout,
            MarkdownLayout::Edit,
            RichText::new(format!("\u{1f4dd} {}", octa::i18n::t("view.md_edit"))).size(15.0),
        );
        if layout != tab.markdown_layout {
            tab.markdown_layout = layout;
        }
    });
    ui.add_space(4.0);

    // Highlight search (always on for this text view): match ranges over the
    // source, the toolbar count, and a pending next/previous jump for the
    // editor pane. The preview pane highlights occurrences via render_pulldown.
    let matcher = (!tab.search_text.is_empty()).then(|| tab.search_matcher());
    let match_ranges: Vec<std::ops::Range<usize>> = matcher
        .as_ref()
        .map(|m| m.find_ranges(&content_owned))
        .unwrap_or_default();
    tab.search_nav.match_count = match_ranges.len();
    if tab.search_nav.current >= match_ranges.len() {
        tab.search_nav.current = 0;
    }
    let jump_dir = tab.search_nav.pending_jump.take();
    if let Some(dir) = jump_dir
        && !match_ranges.is_empty()
    {
        let n = match_ranges.len();
        tab.search_nav.current = match dir {
            NavDir::Next => (tab.search_nav.current + 1) % n,
            NavDir::Prev => (tab.search_nav.current + n - 1) % n,
        };
    }
    let current_range: Option<std::ops::Range<usize>> =
        match_ranges.get(tab.search_nav.current).cloned();
    let editor_search = EditorSearch {
        ranges: &match_ranges,
        current: current_range.as_ref(),
        do_scroll: jump_dir.is_some() && current_range.is_some(),
    };
    let preview_hl: Option<(RowMatcher, Color32)> = matcher.map(|m| (m, MD_HL_NORMAL));

    match tab.markdown_layout {
        MarkdownLayout::Preview => {
            render_preview_pane(ui, tab, &content_owned, preview_hl.as_ref());
        }
        MarkdownLayout::Edit => {
            render_editor_pane(
                ui,
                tab,
                readonly,
                tab_size,
                ui.available_width(),
                &editor_search,
            );
        }
        MarkdownLayout::Split => {
            // 50/50 split. The left SidePanel hosts the editor; the central
            // area receives the rendered preview.
            let editor_width = (ui.available_width() * 0.5).max(200.0);
            egui::Panel::left("md_editor_pane")
                .resizable(true)
                .min_size(150.0)
                .default_size(editor_width)
                .show_inside(ui, |ui| {
                    render_editor_pane(
                        ui,
                        tab,
                        readonly,
                        tab_size,
                        ui.available_width(),
                        &editor_search,
                    );
                });
            render_preview_pane(
                ui,
                tab,
                &tab.raw_content.clone().unwrap_or_default(),
                preview_hl.as_ref(),
            );
        }
    }
}

/// Highlight-search data threaded into the Markdown editor pane: byte ranges of
/// every match, the current (navigated) match, and whether a jump is pending.
struct EditorSearch<'a> {
    ranges: &'a [std::ops::Range<usize>],
    current: Option<&'a std::ops::Range<usize>>,
    do_scroll: bool,
}

fn render_editor_pane(
    ui: &mut egui::Ui,
    tab: &mut TabState,
    readonly: bool,
    tab_size: usize,
    _width: f32,
    search: &EditorSearch<'_>,
) {
    let Some(buffer) = tab.raw_content.as_mut() else {
        return;
    };

    // Line-number gutter, mirroring the Raw view editor
    // (`raw_text::render_raw_view`). Numbers are right-aligned to the widest
    // index and rendered in a non-interactive monospace TextEdit so they share
    // the editor's line height and scroll position. Theme colours come from the
    // active visuals so we needn't thread `theme_mode` through every caller.
    let line_count = buffer.lines().count().max(1);
    let line_num_text: String = (1..=line_count)
        .map(|n| format!("{:>width$}", n, width = line_count.to_string().len()))
        .collect::<Vec<_>>()
        .join("\n");
    let line_num_width = line_count.to_string().len() as f32 * 8.0 + 16.0;
    let mono_font = egui::FontId::new(13.0, egui::FontFamily::Monospace);
    let muted = ui.visuals().weak_text_color();
    let border = ui.visuals().window_stroke().color;

    // `desired_width(f32::INFINITY)` disables auto-wrap so long lines extend
    // beyond the visible pane; the surrounding `ScrollArea::both` then
    // provides horizontal scrolling instead of clipping or word-wrapping.
    let response = egui::ScrollArea::both()
        .id_salt("markdown_editor_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.add_sized(
                    [line_num_width, ui.available_height()],
                    egui::TextEdit::multiline(&mut line_num_text.clone())
                        .font(mono_font.clone())
                        .interactive(false)
                        .desired_width(line_num_width)
                        .text_color(muted)
                        .frame(egui::Frame::NONE),
                );
                ui.add_space(2.0);
                let sep_rect = egui::Rect::from_min_size(
                    ui.cursor().left_top(),
                    egui::vec2(1.0, ui.available_height()),
                );
                ui.painter().rect_filled(sep_rect, 0.0, border);
                ui.add_space(4.0);
                // `lock_focus(true)` keeps Tab inside the editor (it would
                // otherwise move focus to the next widget); we then expand any
                // literal \t egui inserts into `tab_size` spaces, matching the
                // Raw view editor (`raw_text::render_raw_view`).
                let ed_ranges = search.ranges.to_vec();
                let ed_current = search.current.cloned();
                let mut layouter = move |ui: &egui::Ui, text: &dyn egui::TextBuffer, _w: f32| {
                    let mut job = egui::text::LayoutJob::simple(
                        text.as_str().to_owned(),
                        egui::FontId::new(13.0, egui::FontFamily::Monospace),
                        ui.visuals().text_color(),
                        f32::INFINITY,
                    );
                    job.wrap.max_width = f32::INFINITY;
                    crate::ui::search_highlight::apply_highlight(
                        &mut job,
                        &ed_ranges,
                        ed_current.as_ref(),
                        MD_HL_NORMAL,
                        MD_HL_ACTIVE,
                    );
                    ui.fonts_mut(|f| f.layout_job(job))
                };
                let mut output = egui::TextEdit::multiline(buffer)
                    .id(egui::Id::new("markdown_editor"))
                    .font(mono_font)
                    .desired_width(f32::INFINITY)
                    .desired_rows(20)
                    .lock_focus(true)
                    .interactive(!readonly)
                    .layouter(&mut layouter)
                    .show(ui);

                // Follow a selection dragged past the edge of the pane, so it is
                // not capped at the lines currently on screen.
                super::text_ops::autoscroll_while_selecting(ui, &output.response);

                // Replace any inserted \t with spaces and re-anchor the cursor
                // to account for the expansion. Skipped under read-only since
                // `interactive(false)` blocks new insertions.
                let had_tabs = !readonly && buffer.contains('\t');
                if had_tabs {
                    let cursor_idx = output.cursor_range.map(|r| r.primary.index).unwrap_or(0);
                    let tabs_before = buffer[..cursor_idx.min(buffer.len())]
                        .chars()
                        .filter(|&c| c == '\t')
                        .count();
                    let spaces = " ".repeat(tab_size);
                    *buffer = buffer.replace('\t', &spaces);
                    let new_idx = cursor_idx + tabs_before * tab_size.saturating_sub(1);
                    let new_cursor = egui::text::CCursor::new(new_idx);
                    output
                        .state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::one(new_cursor)));
                    // Clone before store so `output.state` stays usable below.
                    output.state.clone().store(ui.ctx(), output.response.id);
                }

                // Highlight-search jump: place the cursor on the current match
                // and scroll it into view.
                if search.do_scroll
                    && let Some(r) = search.current
                {
                    let start_chars = buffer[..r.start].chars().count();
                    let end_chars = buffer[..r.end].chars().count();
                    let ccur_start = egui::text::CCursor::new(start_chars);
                    let ccur_end = egui::text::CCursor::new(end_chars);
                    output
                        .state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::two(ccur_start, ccur_end)));
                    output.state.clone().store(ui.ctx(), output.response.id);
                    let crect = output.galley.pos_from_cursor(ccur_start);
                    let screen = crect.translate(output.galley_pos.to_vec2());
                    ui.scroll_to_rect(screen, Some(egui::Align::Center));
                    output.response.request_focus();
                }
                (output.response, had_tabs)
            })
            .inner
        })
        .inner;
    let (response, had_tabs) = response;
    if (response.changed() || had_tabs) && !readonly {
        tab.raw_content_modified = true;
        tab.markdown_render_cache = None;
    }
}

fn render_preview_pane(
    ui: &mut egui::Ui,
    tab: &mut TabState,
    raw_content: &str,
    hl: Option<&(RowMatcher, Color32)>,
) {
    // CRLF normalization for consistent line handling - pulldown_cmark
    // accepts both, but `\r`-only line endings interact poorly with our
    // event-driven renderer's break heuristics.
    let raw_normalized = if raw_content.contains('\r') {
        raw_content.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        raw_content.to_string()
    };
    let hash = content_hash(&raw_normalized);
    if !matches!(&tab.markdown_render_cache, Some((h, _)) if *h == hash) {
        // Even though we no longer pre-render to HTML-translated CommonMark,
        // keep the cache pointer fresh so other code (e.g. raw editor change
        // handler) can still invalidate it.
        tab.markdown_render_cache = Some((hash, raw_normalized.clone()));
    }

    let bg_response = ui.interact(
        ui.available_rect_before_wrap(),
        ui.id().with("markdown_bg"),
        egui::Sense::click(),
    );
    let raw_for_copy = raw_content.to_string();
    bg_response.context_menu(|ui| {
        if ui.button(octa::i18n::t("view.md_copy")).clicked() {
            ui.ctx().copy_text(raw_for_copy.clone());
            ui.close();
        }
    });

    let pending_offset = tab.markdown_scroll_target.take();
    let mut scroll_area = egui::ScrollArea::both()
        .id_salt("markdown_scroll")
        .auto_shrink([false, false]);
    if let Some(offset) = pending_offset {
        scroll_area = scroll_area.vertical_scroll_offset(offset);
    }
    scroll_area.show(ui, |ui| {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            ui.vertical(|ui| {
                let cap = ui.available_width().clamp(200.0, 900.0);
                ui.set_max_width(cap);
                render_pulldown(ui, &raw_normalized, hl);
            });
        });
    });
}

/// Custom markdown renderer using `pulldown_cmark`. `**bold**` runs use the
/// bundled `FontFamily::Name("bold")` family (registered in `apply_fonts`)
/// instead of egui's color-only `RichText::strong()`.
pub(crate) fn render_pulldown(ui: &mut egui::Ui, src: &str, hl: Option<&(RowMatcher, Color32)>) {
    let bold_body = false;
    use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = Parser::new_ext(src, opts);
    let body_size = 13.0;
    let mut state = InlineState::default();

    // Buffer pending inline runs for the current block. Flushed when the
    // block closes (paragraph/heading/list-item end).
    let mut buf: Vec<(String, RunStyle)> = Vec::new();
    let mut block_kind = BlockKind::Paragraph;
    let mut list_stack: Vec<ListInfo> = Vec::new();
    let mut code_block_buf = String::new();
    // Table state. When `Some`, inline events (`Text`, `Code`, ...) route into
    // the active cell's buffer instead of the outer block buffer. Reset on
    // `TagEnd::Table` after rendering.
    let mut table: Option<TableState> = None;
    let mut table_counter: u64 = 0;

    for event in parser {
        match event {
            Event::Start(tag) => match tag {
                Tag::Paragraph => {
                    block_kind = BlockKind::Paragraph;
                }
                Tag::Heading { level, .. } => {
                    block_kind = BlockKind::Heading(heading_level_u8(level));
                }
                Tag::BlockQuote(_) => {
                    block_kind = BlockKind::Quote;
                }
                Tag::CodeBlock(_) => {
                    block_kind = BlockKind::CodeBlock;
                    code_block_buf.clear();
                }
                Tag::List(start) => {
                    list_stack.push(ListInfo {
                        ordered: start.is_some(),
                        next_num: start.unwrap_or(1),
                    });
                }
                Tag::Item => {
                    block_kind = BlockKind::ListItem;
                }
                Tag::Emphasis => state.italic = true,
                Tag::Strong => state.strong = true,
                Tag::Strikethrough => state.strikethrough = true,
                Tag::Link { dest_url, .. } => {
                    state.link = Some(dest_url.to_string());
                }
                Tag::Table(alignments) => {
                    table = Some(TableState {
                        alignments,
                        header: Vec::new(),
                        rows: Vec::new(),
                        current_row: Vec::new(),
                        current_cell: Vec::new(),
                        in_header: false,
                    });
                }
                Tag::TableHead => {
                    if let Some(t) = table.as_mut() {
                        t.in_header = true;
                        t.current_row.clear();
                    }
                }
                Tag::TableRow => {
                    if let Some(t) = table.as_mut() {
                        t.current_row.clear();
                    }
                }
                Tag::TableCell => {
                    if let Some(t) = table.as_mut() {
                        t.current_cell.clear();
                    }
                }
                _ => {}
            },
            Event::End(end) => match end {
                TagEnd::Paragraph => {
                    flush_block(
                        ui,
                        &mut buf,
                        block_kind,
                        &list_stack,
                        body_size,
                        bold_body,
                        hl,
                    );
                    ui.add_space(6.0);
                }
                TagEnd::Heading(_) => {
                    flush_block(
                        ui,
                        &mut buf,
                        block_kind,
                        &list_stack,
                        body_size,
                        bold_body,
                        hl,
                    );
                    ui.add_space(8.0);
                    block_kind = BlockKind::Paragraph;
                }
                TagEnd::BlockQuote(_) => {
                    flush_block(
                        ui,
                        &mut buf,
                        block_kind,
                        &list_stack,
                        body_size,
                        bold_body,
                        hl,
                    );
                    ui.add_space(4.0);
                    block_kind = BlockKind::Paragraph;
                }
                TagEnd::CodeBlock => {
                    render_code_block(ui, &code_block_buf, body_size, hl);
                    code_block_buf.clear();
                    ui.add_space(6.0);
                    block_kind = BlockKind::Paragraph;
                }
                TagEnd::List(_) => {
                    list_stack.pop();
                }
                TagEnd::Item => {
                    flush_block(
                        ui,
                        &mut buf,
                        BlockKind::ListItem,
                        &list_stack,
                        body_size,
                        bold_body,
                        hl,
                    );
                    if let Some(top) = list_stack.last_mut() {
                        top.next_num += 1;
                    }
                }
                TagEnd::Emphasis => state.italic = false,
                TagEnd::Strong => state.strong = false,
                TagEnd::Strikethrough => state.strikethrough = false,
                TagEnd::Link => state.link = None,
                TagEnd::TableCell => {
                    if let Some(t) = table.as_mut() {
                        let cell = std::mem::take(&mut t.current_cell);
                        t.current_row.push(cell);
                    }
                }
                TagEnd::TableHead => {
                    if let Some(t) = table.as_mut() {
                        t.header = std::mem::take(&mut t.current_row);
                        t.in_header = false;
                    }
                }
                TagEnd::TableRow => {
                    if let Some(t) = table.as_mut()
                        && !t.in_header
                    {
                        let row = std::mem::take(&mut t.current_row);
                        t.rows.push(row);
                    }
                }
                TagEnd::Table => {
                    if let Some(t) = table.take() {
                        table_counter += 1;
                        render_table(ui, &t, body_size, table_counter);
                        ui.add_space(6.0);
                    }
                }
                _ => {}
            },
            Event::Text(text) => {
                if let Some(t) = table.as_mut() {
                    t.current_cell.push((text.into_string(), state.style()));
                } else if matches!(block_kind, BlockKind::CodeBlock) {
                    code_block_buf.push_str(&text);
                } else {
                    buf.push((text.into_string(), state.style()));
                }
            }
            Event::Code(text) => {
                let mut s = state.style();
                s.code = true;
                if let Some(t) = table.as_mut() {
                    t.current_cell.push((text.into_string(), s));
                } else {
                    buf.push((text.into_string(), s));
                }
            }
            Event::SoftBreak => {
                if let Some(t) = table.as_mut() {
                    t.current_cell.push((" ".to_string(), state.style()));
                } else {
                    buf.push((" ".to_string(), state.style()));
                }
            }
            Event::HardBreak => {
                if let Some(t) = table.as_mut() {
                    t.current_cell.push(("\n".to_string(), state.style()));
                } else {
                    buf.push(("\n".to_string(), state.style()));
                }
            }
            Event::Rule => {
                flush_block(
                    ui,
                    &mut buf,
                    block_kind,
                    &list_stack,
                    body_size,
                    bold_body,
                    hl,
                );
                ui.separator();
                ui.add_space(4.0);
            }
            _ => {}
        }
    }
    flush_block(
        ui,
        &mut buf,
        block_kind,
        &list_stack,
        body_size,
        bold_body,
        hl,
    );
}

#[derive(Default, Clone)]
struct InlineState {
    italic: bool,
    strong: bool,
    strikethrough: bool,
    link: Option<String>,
}

impl InlineState {
    fn style(&self) -> RunStyle {
        RunStyle {
            italic: self.italic,
            strong: self.strong,
            strikethrough: self.strikethrough,
            code: false,
            link: self.link.clone(),
        }
    }
}

#[derive(Default, Clone)]
struct RunStyle {
    italic: bool,
    strong: bool,
    strikethrough: bool,
    code: bool,
    link: Option<String>,
}

#[derive(Clone, Copy)]
enum BlockKind {
    Paragraph,
    Heading(u8),
    Quote,
    CodeBlock,
    ListItem,
}

struct ListInfo {
    ordered: bool,
    next_num: u64,
}

fn heading_level_u8(level: pulldown_cmark::HeadingLevel) -> u8 {
    use pulldown_cmark::HeadingLevel as H;
    match level {
        H::H1 => 1,
        H::H2 => 2,
        H::H3 => 3,
        H::H4 => 4,
        H::H5 => 5,
        H::H6 => 6,
    }
}

fn flush_block(
    ui: &mut egui::Ui,
    buf: &mut Vec<(String, RunStyle)>,
    kind: BlockKind,
    list_stack: &[ListInfo],
    body_size: f32,
    bold_body: bool,
    hl: Option<&(RowMatcher, Color32)>,
) {
    if buf.is_empty() {
        return;
    }
    let runs = std::mem::take(buf);

    match kind {
        BlockKind::Heading(level) => {
            let size = match level {
                1 => body_size * 1.8,
                2 => body_size * 1.5,
                3 => body_size * 1.3,
                4 => body_size * 1.15,
                _ => body_size * 1.05,
            };
            render_runs(ui, &runs, size, /* force_bold */ true, hl);
        }
        BlockKind::Paragraph => {
            render_runs(ui, &runs, body_size, bold_body, hl);
        }
        BlockKind::Quote => {
            ui.horizontal_wrapped(|ui| {
                ui.add_space(12.0);
                let muted = ui.visuals().weak_text_color();
                ui.label(RichText::new("\u{2503}").color(muted));
                ui.add_space(6.0);
                render_runs(ui, &runs, body_size, bold_body, hl);
            });
        }
        BlockKind::ListItem => {
            ui.horizontal_wrapped(|ui| {
                let depth = list_stack.len().saturating_sub(1);
                ui.add_space(8.0 + depth as f32 * 16.0);
                let bullet = match list_stack.last() {
                    Some(li) if li.ordered => format!("{}. ", li.next_num),
                    _ => "\u{2022} ".to_string(),
                };
                let bullet_family = if bold_body {
                    egui::FontFamily::Name(std::sync::Arc::from("bold"))
                } else {
                    egui::FontFamily::Proportional
                };
                ui.label(RichText::new(bullet).font(egui::FontId::new(body_size, bullet_family)));
                render_runs(ui, &runs, body_size, bold_body, hl);
            });
        }
        BlockKind::CodeBlock => { /* handled separately */ }
    }
}

/// Collect the char ranges covered by link runs, paired with their URL. Char
/// (not byte) ranges so they line up with `Galley::cursor_from_pos`, which
/// returns a char index.
fn link_spans(runs: &[(String, RunStyle)]) -> Vec<(std::ops::Range<usize>, String)> {
    let mut spans = Vec::new();
    let mut off = 0usize;
    for (text, style) in runs {
        let len = text.chars().count();
        if let Some(url) = &style.link {
            spans.push((off..off + len, url.clone()));
        }
        off += len;
    }
    spans
}

/// Given a laid-out galley for a styled label and the link spans within it,
/// show a pointer cursor over links and open the link under the pointer on
/// click. External URLs open in the system browser; in-document `#fragment`
/// targets are ignored (anchor scrolling is not implemented).
fn open_link_under_pointer(
    ui: &egui::Ui,
    resp: &egui::Response,
    galley: &egui::Galley,
    spans: &[(std::ops::Range<usize>, String)],
) {
    let idx_at = |pos: egui::Pos2| galley.cursor_from_pos(pos - resp.rect.min).index;
    if let Some(hover) = resp.hover_pos()
        && spans.iter().any(|(r, _)| r.contains(&idx_at(hover)))
    {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked()
        && let Some(pos) = resp.interact_pointer_pos()
    {
        let idx = idx_at(pos);
        if let Some((_, url)) = spans.iter().find(|(r, _)| r.contains(&idx))
            && !url.starts_with('#')
        {
            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
        }
    }
}

/// Build a `LayoutJob` from a list of styled runs and emit it as a wrapping
/// `Label`. Bold runs use the bundled `FontFamily::Name("bold")` family;
/// italics use egui's runtime skew; code uses Monospace + a tinted bg.
/// `force_bold` is set by headings and by callers that opted into bold body.
fn render_runs(
    ui: &mut egui::Ui,
    runs: &[(String, RunStyle)],
    size: f32,
    force_bold: bool,
    hl: Option<&(RowMatcher, Color32)>,
) {
    use egui::text::{LayoutJob, TextFormat};
    let mut job = LayoutJob::default();
    job.wrap.max_width = ui.available_width();

    let body_color = ui.visuals().text_color();
    let link_color = ui.visuals().hyperlink_color;
    let bold_family = egui::FontFamily::Name(std::sync::Arc::from("bold"));

    for (text, style) in runs {
        let mut fmt = TextFormat::default();
        let want_bold = style.strong || force_bold;
        let family = if style.code {
            egui::FontFamily::Monospace
        } else if want_bold {
            bold_family.clone()
        } else {
            egui::FontFamily::Proportional
        };
        fmt.font_id = egui::FontId::new(size, family);
        fmt.color = if style.link.is_some() {
            link_color
        } else {
            body_color
        };
        fmt.italics = style.italic;
        if style.strikethrough {
            fmt.strikethrough = egui::Stroke::new(1.0_f32, body_color);
        }
        if style.link.is_some() {
            fmt.underline = egui::Stroke::new(1.0_f32, link_color);
        }
        if style.code {
            fmt.background = ui.visuals().code_bg_color;
        }
        job.append(text, 0.0, fmt);
    }

    let full_text: String = runs.iter().map(|(t, _)| t.as_str()).collect();
    highlight_md_job(&mut job, &full_text, hl);

    let spans = link_spans(runs);
    if spans.is_empty() {
        ui.add(egui::Label::new(job).wrap());
    } else {
        // Lay the job out ourselves so click positions map onto the same galley.
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        let resp = ui.add(egui::Label::new(galley.clone()).sense(egui::Sense::click()));
        open_link_under_pointer(ui, &resp, &galley, &spans);
    }
}

/// Buffered table being collected during pulldown_cmark event traversal.
/// Each cell is a `Vec<(String, RunStyle)>` - the same shape the inline-run
/// flusher already understands, so styling (bold, italic, code, links)
/// inside cells reuses the existing pipeline.
struct TableState {
    alignments: Vec<pulldown_cmark::Alignment>,
    header: Vec<Vec<(String, RunStyle)>>,
    rows: Vec<Vec<Vec<(String, RunStyle)>>>,
    /// Cells of the row currently being built. Becomes `header` on
    /// `TagEnd::TableHead` and gets pushed into `rows` on `TagEnd::TableRow`.
    current_row: Vec<Vec<(String, RunStyle)>>,
    /// Inline runs of the cell currently being built. Pushed into
    /// `current_row` on `TagEnd::TableCell`.
    current_cell: Vec<(String, RunStyle)>,
    /// True while we're between `Tag::TableHead` and its matching `TagEnd`.
    /// Used to keep the header row out of `rows`.
    in_header: bool,
}

/// Render a buffered markdown table.
///
/// Layout: outer `Frame` with a thin border, then manual rows of fixed-width
/// cells (no `egui::Grid` - Grid auto-sizes columns from widest content,
/// which on prose-heavy docs ends up with one column hogging the whole
/// width). The header row gets a faint bg tint and a divider underneath;
/// body rows zebra-stripe via per-row `Frame::fill`. Cells respect the
/// `:---:` / `:---` / `---:` alignment markers via `Label::halign`.
fn render_table(ui: &mut egui::Ui, table: &TableState, body_size: f32, table_id: u64) {
    let num_cols = table
        .header
        .len()
        .max(table.rows.iter().map(|r| r.len()).max().unwrap_or(0));
    if num_cols == 0 {
        return;
    }

    // Spread columns evenly across the available width, but cap the
    // shrink so very narrow tables don't squish below readability. Long
    // cells wrap inside their column via `Label::wrap()`.
    let avail = ui.available_width();
    let inner_pad = 8.0; // per-cell horizontal padding
    let usable = (avail - 8.0).max(120.0); // outer frame padding
    let col_width = ((usable / num_cols as f32) - inner_pad).max(60.0);

    let visuals = ui.visuals();
    let border = visuals.widgets.noninteractive.bg_stroke;
    let header_bg = visuals.faint_bg_color;
    let stripe_bg = visuals.faint_bg_color;
    let body_bg = visuals.panel_fill;

    let outer = egui::Frame::new()
        .stroke(border)
        .corner_radius(4.0)
        .inner_margin(egui::Margin::same(0));

    let ctx = TableLayoutCtx {
        alignments: &table.alignments,
        body_size,
        col_width,
        num_cols,
        table_id,
    };

    outer.show(ui, |ui| {
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);

        // Header row.
        egui::Frame::new()
            .fill(header_bg)
            .inner_margin(egui::Margin::symmetric(4, 4))
            .show(ui, |ui| {
                render_table_row(ui, &table.header, &ctx, /* bold */ true, 0);
            });
        // Divider under the header.
        ui.add(egui::Separator::default().horizontal().spacing(0.0));

        // Body rows, zebra-striped.
        for (row_idx, row) in table.rows.iter().enumerate() {
            let fill = if row_idx % 2 == 0 { body_bg } else { stripe_bg };
            egui::Frame::new()
                .fill(fill)
                .inner_margin(egui::Margin::symmetric(4, 4))
                .show(ui, |ui| {
                    render_table_row(ui, row, &ctx, /* bold */ false, row_idx + 1);
                });
        }
    });
}

/// Shared layout context for every row in a single table - keeps the
/// signature of `render_table_row` short.
struct TableLayoutCtx<'a> {
    alignments: &'a [pulldown_cmark::Alignment],
    body_size: f32,
    col_width: f32,
    num_cols: usize,
    table_id: u64,
}

fn render_table_row(
    ui: &mut egui::Ui,
    cells: &[Vec<(String, RunStyle)>],
    ctx: &TableLayoutCtx<'_>,
    bold: bool,
    row_idx: usize,
) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 0.0);
        for col_idx in 0..ctx.num_cols {
            let halign = match ctx.alignments.get(col_idx).copied() {
                Some(pulldown_cmark::Alignment::Center) => egui::Align::Center,
                Some(pulldown_cmark::Alignment::Right) => egui::Align::Max,
                _ => egui::Align::Min,
            };
            let empty: Vec<(String, RunStyle)> = Vec::new();
            let runs = cells.get(col_idx).unwrap_or(&empty);
            let cell_id = egui::Id::new((
                "md_table_cell",
                ctx.table_id,
                row_idx as u64,
                col_idx as u64,
            ));
            ui.push_id(cell_id, |ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(ctx.col_width, 0.0),
                    egui::Layout::top_down(halign),
                    |ui| {
                        ui.set_min_width(ctx.col_width);
                        ui.set_max_width(ctx.col_width);
                        render_cell_runs(ui, runs, ctx.body_size, bold, halign, ctx.col_width);
                    },
                );
            });
        }
    });
}

/// Layout the inline runs of a single cell. Uses the same bold / italic /
/// code / link affordances `render_runs` does, but with a wrap width tied
/// to the cell column and a horizontal alignment from the `:---:` syntax.
fn render_cell_runs(
    ui: &mut egui::Ui,
    runs: &[(String, RunStyle)],
    size: f32,
    force_bold: bool,
    halign: egui::Align,
    max_width: f32,
) {
    use egui::text::{LayoutJob, TextFormat};

    let mut job = LayoutJob::default();
    job.wrap.max_width = max_width;
    job.halign = halign;

    let body_color = ui.visuals().text_color();
    let link_color = ui.visuals().hyperlink_color;
    let bold_family = egui::FontFamily::Name(std::sync::Arc::from("bold"));

    for (text, style) in runs {
        let mut fmt = TextFormat::default();
        let want_bold = style.strong || force_bold;
        let family = if style.code {
            egui::FontFamily::Monospace
        } else if want_bold {
            bold_family.clone()
        } else {
            egui::FontFamily::Proportional
        };
        fmt.font_id = egui::FontId::new(size, family);
        fmt.color = if style.link.is_some() {
            link_color
        } else {
            body_color
        };
        fmt.italics = style.italic;
        if style.strikethrough {
            fmt.strikethrough = egui::Stroke::new(1.0_f32, body_color);
        }
        if style.link.is_some() {
            fmt.underline = egui::Stroke::new(1.0_f32, link_color);
        }
        if style.code {
            fmt.background = ui.visuals().code_bg_color;
        }
        job.append(text, 0.0, fmt);
    }

    let spans = link_spans(runs);
    if spans.is_empty() {
        ui.add(egui::Label::new(job).halign(halign).wrap());
    } else {
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        let resp = ui.add(egui::Label::new(galley.clone()).sense(egui::Sense::click()));
        open_link_under_pointer(ui, &resp, &galley, &spans);
    }
}

fn render_code_block(
    ui: &mut egui::Ui,
    content: &str,
    size: f32,
    hl: Option<&(RowMatcher, Color32)>,
) {
    let bg = ui.visuals().code_bg_color;
    let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
    egui::Frame::new()
        .fill(bg)
        .stroke(stroke)
        .corner_radius(4.0)
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            let text = content.trim_end_matches('\n');
            let mut job = egui::text::LayoutJob::single_section(
                text.to_string(),
                egui::text::TextFormat {
                    font_id: egui::FontId::new(size, egui::FontFamily::Monospace),
                    color: ui.visuals().text_color(),
                    ..Default::default()
                },
            );
            job.wrap.max_width = ui.available_width();
            highlight_md_job(&mut job, text, hl);
            ui.add(egui::Label::new(job).selectable(true));
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str, link: Option<&str>) -> (String, RunStyle) {
        (
            text.to_string(),
            RunStyle {
                link: link.map(|s| s.to_string()),
                ..Default::default()
            },
        )
    }

    #[test]
    fn link_spans_finds_char_ranges() {
        // "foo " | "bar"(link) | " baz"  ->  link covers chars 4..7.
        let runs = vec![
            run("foo ", None),
            run("bar", Some("https://example.com")),
            run(" baz", None),
        ];
        let spans = link_spans(&runs);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].0, 4..7);
        assert_eq!(spans[0].1, "https://example.com");
    }

    #[test]
    fn link_spans_empty_when_no_links() {
        let runs = vec![run("plain text", None)];
        assert!(link_spans(&runs).is_empty());
    }
}
