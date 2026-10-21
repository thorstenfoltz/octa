//! Several carets in one text editor, the way VS Code has them: type, delete
//! or paste once and it happens at every caret. egui's `TextEdit` knows one
//! cursor, so the extra carets live here, in egui's memory under the editor's
//! id, and [`MultiCursor::before`] applies the keys to all of them before the
//! editor sees them. Used by the SQL editor, the Raw view and Markdown Edit.
//!
//! Adding carets: Alt+click (again on a caret removes it); Shift+Alt+Up/Down,
//! a caret on the line above or below in the same column;
//! Shift+Alt+I, one at the end of every line of the selection. Those are the
//! defaults: the keys are in Settings -> Shortcuts (Text editors) and the
//! click in Settings -> Search & Editor, handed over by [`set_config`].
//! Escape, a plain click, or a key this module does not do for every caret
//! (Ctrl+Z, Shift+arrow, Ctrl+A...) goes back to one caret.
//!
//! ponytail: carets only, no selection per caret, so Shift+arrow or a word
//! jump drops back to one caret; add per-caret selections if people want to
//! replace a block column-wise.

use eframe::egui;
use egui::text::{CCursor, CCursorRange};
use octa::ui::settings::CaretClick;
use octa::ui::shortcuts::{KeyCombo, ShortcutAction as SA, Shortcuts};

/// What adds a caret, from the settings.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CaretConfig {
    above: KeyCombo,
    below: KeyCombo,
    line_ends: KeyCombo,
    click: CaretClick,
}

impl CaretConfig {
    pub(crate) fn new(shortcuts: &Shortcuts, click: CaretClick) -> Self {
        let key = |a| shortcuts.combo(a);
        Self {
            above: key(SA::AddCaretAbove),
            below: key(SA::AddCaretBelow),
            line_ends: key(SA::CaretsAtLineEnds),
            click,
        }
    }

    fn click_held(&self, m: egui::Modifiers) -> bool {
        match self.click {
            CaretClick::Alt => m.alt,
            CaretClick::Ctrl => m.command,
            CaretClick::Off => false,
        }
    }
}

fn config_key() -> egui::Id {
    egui::Id::new("octa_multi_caret_config")
}

/// Hand the settings to every editor of `ctx`. The app does it each frame;
/// a context that never got one (tests) uses the shipped defaults.
pub(crate) fn set_config(ctx: &egui::Context, config: CaretConfig) {
    ctx.data_mut(|d| d.insert_temp(config_key(), config));
}

fn load_config(ctx: &egui::Context) -> CaretConfig {
    ctx.data(|d| d.get_temp(config_key())).unwrap_or_else(|| {
        // No bindings stored: every action falls back to its default.
        let defaults = Shortcuts {
            bindings: Default::default(),
        };
        CaretConfig::new(&defaults, CaretClick::default())
    })
}

/// One edit made at every caret.
#[derive(Debug, Clone, PartialEq)]
enum Edit<'a> {
    Insert(&'a str),
    /// One piece per caret, top to bottom: a paste with as many lines as
    /// there are carets puts one line at each.
    InsertEach(Vec<&'a str>),
    Backspace,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Move {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
}

/// `edit` at every caret (char indices, sorted, distinct). Returns the new
/// text and where each caret ends up, in the same order.
fn apply(text: &str, carets: &[usize], edit: &Edit) -> (String, Vec<usize>) {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut out_chars = 0;
    let copy = |out: &mut String, from: usize, to: usize, n: &mut usize| {
        out.extend(&chars[from..to]);
        *n += to - from;
    };
    let mut at = 0;
    let mut moved = Vec::with_capacity(carets.len());
    for (k, &c) in carets.iter().enumerate() {
        let c = c.clamp(at, chars.len());
        match edit {
            Edit::Backspace => {
                copy(&mut out, at, c.saturating_sub(1).max(at), &mut out_chars);
                at = c;
            }
            Edit::Delete => {
                copy(&mut out, at, c, &mut out_chars);
                at = (c + 1).min(chars.len());
            }
            Edit::Insert(s) => {
                copy(&mut out, at, c, &mut out_chars);
                out.push_str(s);
                out_chars += s.chars().count();
                at = c;
            }
            Edit::InsertEach(pieces) => {
                copy(&mut out, at, c, &mut out_chars);
                let s = pieces.get(k).copied().unwrap_or("");
                out.push_str(s);
                out_chars += s.chars().count();
                at = c;
            }
        }
        moved.push(out_chars);
    }
    copy(&mut out, at, chars.len(), &mut out_chars);
    (out, moved)
}

/// Where the line holding char `c` starts.
fn line_start(chars: &[char], c: usize) -> usize {
    chars[..c.min(chars.len())]
        .iter()
        .rposition(|&ch| ch == '\n')
        .map_or(0, |i| i + 1)
}

/// Where the line starting at `start` ends (its `\n`, or the end of text).
fn line_end(chars: &[char], start: usize) -> usize {
    chars[start..]
        .iter()
        .position(|&ch| ch == '\n')
        .map_or(chars.len(), |i| start + i)
}

/// Column `col` of the line starting at `start`, or its end when shorter.
fn at_col(chars: &[char], start: usize, col: usize) -> usize {
    start + col.min(line_end(chars, start) - start)
}

/// The line above or below the one holding `c`, at column `col`; `None`
/// past the first or last line.
fn other_line(chars: &[char], c: usize, col: usize, down: bool) -> Option<usize> {
    let start = line_start(chars, c);
    if down {
        let end = line_end(chars, start);
        (end < chars.len()).then(|| at_col(chars, end + 1, col))
    } else {
        (start > 0).then(|| at_col(chars, line_start(chars, start - 1), col))
    }
}

fn step(chars: &[char], c: usize, m: Move) -> usize {
    let c = c.min(chars.len());
    let start = line_start(chars, c);
    match m {
        Move::Left => c.saturating_sub(1),
        Move::Right => (c + 1).min(chars.len()),
        Move::Home => start,
        Move::End => line_end(chars, start),
        Move::Up => other_line(chars, c, c - start, false).unwrap_or(0),
        Move::Down => other_line(chars, c, c - start, true).unwrap_or(chars.len()),
    }
}

/// The end of every line the selection `from..to` touches. A selection
/// ending at the very start of a line leaves that line out, as a selection
/// of whole lines does.
fn line_ends(chars: &[char], from: usize, to: usize) -> Vec<usize> {
    let last = if to > from && line_start(chars, to) == to {
        to - 1
    } else {
        to
    };
    let mut out = Vec::new();
    let mut start = line_start(chars, from);
    loop {
        let end = line_end(chars, start);
        out.push(end);
        if end >= last || end >= chars.len() {
            return out;
        }
        start = end + 1;
    }
}

/// The carets as one sorted list plus where the primary sits in it.
fn all_carets(primary: usize, extras: &[usize]) -> (Vec<usize>, usize) {
    let mut all: Vec<usize> = extras.iter().copied().chain([primary]).collect();
    all.sort_unstable();
    all.dedup();
    let p = all.binary_search(&primary).unwrap_or(0);
    (all, p)
}

/// Split `moved` back into the primary and the extras (distinct, without
/// the primary).
fn split_carets(moved: Vec<usize>, p: usize) -> (usize, Vec<usize>) {
    let primary = moved[p];
    let mut extras: Vec<usize> = moved.into_iter().filter(|&c| c != primary).collect();
    extras.dedup();
    (primary, extras)
}

fn chars_of<'a>(cache: &'a mut Option<Vec<char>>, text: &str) -> &'a [char] {
    cache.get_or_insert_with(|| text.chars().collect())
}

fn memory_key(id: egui::Id) -> egui::Id {
    id.with("octa_multi_carets")
}

fn load_extras(ctx: &egui::Context, id: egui::Id) -> Vec<usize> {
    ctx.data(|d| d.get_temp::<Vec<usize>>(memory_key(id)))
        .unwrap_or_default()
}

fn store_extras(ctx: &egui::Context, id: egui::Id, extras: Vec<usize>) {
    ctx.data_mut(|d| d.insert_temp(memory_key(id), extras));
}

/// Whether the editor `id` has more than one caret right now.
pub(crate) fn has_extras(ctx: &egui::Context, id: egui::Id) -> bool {
    !load_extras(ctx, id).is_empty()
}

/// The keys the editor `id` keeps while focused: Tab indents, arrows move,
/// and with extra carets Escape goes back to one caret instead of leaving the
/// editor (egui drops focus on an Escape the filter does not claim before any
/// widget sees it). What `lock_focus(true)` gives, plus that.
pub(crate) fn event_filter(ctx: &egui::Context, id: egui::Id) -> egui::EventFilter {
    egui::EventFilter {
        tab: true,
        horizontal_arrows: true,
        vertical_arrows: true,
        escape: has_extras(ctx, id),
    }
}

/// What [`MultiCursor::before`] found, for [`MultiCursor::after`].
pub(crate) struct MultiCursor {
    id: egui::Id,
    /// The primary caret before the editor ran, for the caret click.
    primary_before: Option<usize>,
    /// The text was changed here, at every caret.
    edited: bool,
}

impl MultiCursor {
    /// Call right before the `TextEdit` with id `id` over `text` is shown.
    /// `tab` is what the Tab key types; `enabled` is false for a read-only
    /// editor, which then has one caret.
    pub(crate) fn before(
        ui: &egui::Ui,
        id: egui::Id,
        text: &mut String,
        tab: &str,
        enabled: bool,
    ) -> Self {
        let ctx = ui.ctx().clone();
        let mut this = Self {
            id,
            primary_before: None,
            edited: false,
        };
        let mut extras = load_extras(&ctx, id);
        if !enabled {
            if !extras.is_empty() {
                store_extras(&ctx, id, Vec::new());
            }
            return this;
        }
        let Some(mut state) = egui::TextEdit::load_state(&ctx, id) else {
            return this;
        };
        let Some(range) = state.cursor.char_range() else {
            return this;
        };
        let mut primary = range.primary.index.0;
        this.primary_before = Some(primary);
        if !ctx.memory(|m| m.has_focus(id)) {
            return this;
        }

        let config = load_config(&ctx);
        let events = ctx.input_mut(|i| std::mem::take(&mut i.events));
        let mut kept = Vec::with_capacity(events.len());
        // Built on the first event that needs it, never per frame: a mouse
        // move over a 50 MB buffer must not walk it.
        let mut cache: Option<Vec<char>> = None;
        let mut cursor_moved = false;
        for event in events {
            match &event {
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => {
                    use egui::Key;
                    let down = config.below.matches(*key, *modifiers);
                    if down || config.above.matches(*key, *modifiers) {
                        let chars = chars_of(&mut cache, text);
                        let (all, _) = all_carets(primary, &extras);
                        let edge = if down { all[all.len() - 1] } else { all[0] };
                        let p0 = primary.min(chars.len());
                        let col = p0 - line_start(chars, p0);
                        if let Some(c) = other_line(chars, edge, col, down)
                            && !all.contains(&c)
                        {
                            extras.push(c);
                        }
                        continue;
                    }
                    if config.line_ends.matches(*key, *modifiers) {
                        let chars = chars_of(&mut cache, text);
                        let (a, b) = (range.primary.index.0, range.secondary.index.0);
                        let from = a.min(b).min(chars.len());
                        let to = a.max(b).min(chars.len());
                        let mut ends = line_ends(chars, from, to);
                        primary = ends.pop().unwrap_or(primary);
                        extras = ends;
                        cursor_moved = true;
                        continue;
                    }
                    if extras.is_empty() {
                        kept.push(event);
                        continue;
                    }
                    let plain = modifiers.is_none();
                    let edit = match key {
                        Key::Escape => {
                            extras.clear();
                            continue;
                        }
                        Key::Backspace if plain => Some(Edit::Backspace),
                        Key::Delete if plain => Some(Edit::Delete),
                        Key::Enter if plain || *modifiers == egui::Modifiers::SHIFT => {
                            Some(Edit::Insert("\n"))
                        }
                        Key::Tab if plain => Some(Edit::Insert(tab)),
                        _ => None,
                    };
                    let mv = match key {
                        _ if !plain => None,
                        Key::ArrowLeft => Some(Move::Left),
                        Key::ArrowRight => Some(Move::Right),
                        Key::ArrowUp => Some(Move::Up),
                        Key::ArrowDown => Some(Move::Down),
                        Key::Home => Some(Move::Home),
                        Key::End => Some(Move::End),
                        _ => None,
                    };
                    if let Some(edit) = edit {
                        let (all, p) = all_carets(primary, &extras);
                        let (out, moved) = apply(text, &all, &edit);
                        *text = out;
                        cache = None;
                        (primary, extras) = split_carets(moved, p);
                        this.edited = true;
                    } else if let Some(mv) = mv {
                        let chars = chars_of(&mut cache, text);
                        let (all, p) = all_carets(primary, &extras);
                        let moved = all.iter().map(|&c| step(chars, c, mv)).collect();
                        (primary, extras) = split_carets(moved, p);
                        cursor_moved = true;
                    } else {
                        // A letter's key event comes with its Text event and
                        // does nothing itself; Ctrl+Enter runs the SQL. Any
                        // other key is the editor's, for its one caret.
                        let typing = !modifiers.command
                            && !modifiers.ctrl
                            && !modifiers.alt
                            && !matches!(key, Key::PageUp | Key::PageDown)
                            && !(modifiers.shift
                                && matches!(
                                    key,
                                    Key::ArrowLeft
                                        | Key::ArrowRight
                                        | Key::ArrowUp
                                        | Key::ArrowDown
                                        | Key::Home
                                        | Key::End
                                        | Key::Tab
                                ));
                        let runs_sql = *key == Key::Enter && modifiers.command;
                        if !typing && !runs_sql {
                            extras.clear();
                        }
                        kept.push(event);
                    }
                    continue;
                }
                egui::Event::Text(s)
                    if !extras.is_empty() && !s.is_empty() && s != "\n" && s != "\r" =>
                {
                    let (all, p) = all_carets(primary, &extras);
                    let (out, moved) = apply(text, &all, &Edit::Insert(s));
                    *text = out;
                    cache = None;
                    (primary, extras) = split_carets(moved, p);
                    this.edited = true;
                    continue;
                }
                egui::Event::Paste(s) if !extras.is_empty() && !s.is_empty() => {
                    let s = s.replace("\r\n", "\n");
                    let (all, p) = all_carets(primary, &extras);
                    let lines: Vec<&str> = s.trim_end_matches('\n').split('\n').collect();
                    let edit = if lines.len() == all.len() {
                        Edit::InsertEach(lines)
                    } else {
                        Edit::Insert(&s)
                    };
                    let (out, moved) = apply(text, &all, &edit);
                    *text = out;
                    cache = None;
                    (primary, extras) = split_carets(moved, p);
                    this.edited = true;
                    continue;
                }
                _ => {}
            }
            kept.push(event);
        }
        ctx.input_mut(|i| i.events = kept);

        if this.edited || cursor_moved {
            state
                .cursor
                .set_char_range(Some(CCursorRange::one(CCursor::new(primary))));
            state.store(&ctx, id);
            this.primary_before = Some(primary);
        }
        store_extras(&ctx, id, extras);
        this
    }

    /// Call right after the editor was shown, with what `show` returned.
    pub(crate) fn after(self, ui: &egui::Ui, output: &mut egui::text_edit::TextEditOutput) {
        let ctx = ui.ctx();
        let mut extras = load_extras(ctx, self.id);
        // The editor changed the text itself (undo, cut, an IME): the
        // extra carets no longer point where they did.
        let response = &mut output.response.response;
        if response.changed() && !self.edited {
            extras.clear();
        }
        if self.edited {
            response.mark_changed();
        }
        if response.hovered() && ui.input(|i| i.pointer.primary_pressed()) {
            let config = load_config(ctx);
            let held = ui.input(|i| config.click_held(i.modifiers));
            // Not `output.cursor_range`: egui fills that in before the
            // click moves the caret, so it still holds the old place.
            let now = output.state.cursor.char_range().map(|r| r.primary.index.0);
            match (held, self.primary_before, now) {
                (true, Some(before), Some(now)) if extras.contains(&now) => {
                    // The caret click on a caret takes it away again.
                    extras.retain(|&c| c != now);
                    output
                        .state
                        .cursor
                        .set_char_range(Some(CCursorRange::one(CCursor::new(before))));
                    output.state.clone().store(ctx, self.id);
                }
                (true, Some(before), Some(now)) if before != now => extras.push(before),
                (true, ..) => {}
                (false, ..) => extras.clear(),
            }
        }
        if !extras.is_empty() && ui.memory(|m| m.has_focus(self.id)) {
            let stroke = ui.visuals().text_cursor.stroke;
            let painter = ui.painter();
            // Past the end clamps to the end, so a caret left behind by
            // text that shrank elsewhere still paints somewhere sensible.
            for &c in &extras {
                let r = output
                    .galley
                    .pos_from_cursor(CCursor::new(c))
                    .translate(output.galley_pos.to_vec2());
                painter.line_segment([r.left_top(), r.left_bottom()], stroke);
            }
        }
        store_extras(ctx, self.id, extras);
    }
}

#[cfg(test)]
#[path = "multi_cursor_tests.rs"]
mod tests;
