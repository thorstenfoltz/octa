//! Who owns Ctrl+C this frame.
//!
//! Octa's table takes over the clipboard shortcut so Ctrl+C copies the marked
//! cells, and it writes the OS clipboard directly (`app::clipboard::do_copy`).
//! That is right when the table is what the user marked, and wrong the moment
//! the marked thing is *text*: a chat bubble, tool output, a message in a
//! dialog, the contents of a focused text box. Those are ordinary egui
//! selections, and egui copies them itself at the end of the pass.
//!
//! So every clipboard-hijacking site asks this first. When it answers true the
//! hijacker must stand down completely: no draining of `Event::Copy` (egui's
//! own copy still needs it), and no clipboard write of its own (which would
//! race egui's). Clicking anywhere outside the text clears the selection, so
//! the table gets its shortcut back with the same gesture that stops the text
//! looking selected.

/// Is any text marked right now: a label selection in any viewport, or a
/// non-empty selection inside the focused text box?
pub fn has_active_selection(ctx: &egui::Context) -> bool {
    if ctx
        .plugin::<egui::text_selection::LabelSelectionState>()
        .lock()
        .has_selection()
    {
        return true;
    }
    ctx.memory(|m| m.focused())
        .and_then(|id| egui::TextEdit::load_state(ctx, id))
        .and_then(|state| state.cursor.char_range())
        .is_some_and(|range| !range.is_empty())
}

/// Did the user ask to copy this frame, with nobody else laying claim to it?
///
/// For the widgets that answer Ctrl+C themselves (JSON tree, compare views):
/// true when an `Event::Copy` is in the queue and no marked text owns it.
///
/// Checking the event rather than `Key::C` matters: the winit layer turns a
/// copy chord into `Event::Copy` and returns, so **no key event is emitted at
/// all** and a `key_pressed(Key::C)` test never fires. A remapped Copy binding
/// is normalised into the same event before any panel renders (see
/// `app::shortcuts_dispatch`), so one check covers both.
pub fn copy_pressed(ctx: &egui::Context) -> bool {
    if has_active_selection(ctx) {
        return false;
    }
    ctx.input(|i| {
        i.events
            .iter()
            .any(|e| matches!(e, egui::Event::Copy | egui::Event::Cut))
    })
}
