//! Greyed, never hidden (user, 2026-10-31). Scans the menu builders and the
//! Settings dialog for an entry hidden by state:
//!   (a) `if <state> && ui.button(..)` - the entry vanishes by short-circuit;
//!   (b) `if <state> { <widget without add_enabled> }`;
//!   (c) a `menu.need_table` early return, which hid a whole menu.
//! An `if` that tests an event (`.clicked()`, `.changed()`, ...) is not state.
//! Exceptions are in ALLOWED with a reason, so a new hidden entry has to be
//! argued for.

use std::path::{Path, PathBuf};

const SCANNED: &[&str] = &[
    "src/ui/toolbar/analyse_menu.rs",
    "src/ui/toolbar/columns_menu.rs",
    "src/ui/toolbar/data_menu.rs",
    "src/ui/toolbar/edit_menu.rs",
    "src/ui/toolbar/file_menu.rs",
    "src/ui/toolbar/help_menu.rs",
    "src/ui/toolbar/search_menu.rs",
    "src/ui/toolbar/view_menu.rs",
    "src/ui/table_view/rows.rs",
    "src/ui/table_view/header.rs",
    "src/app/tabs/tab_bar.rs",
    "src/ui/directory_tree.rs",
    "src/app/db_tree.rs",
    "src/app/cloud_tree.rs",
    "src/ui/settings/dialog",
];

/// Calls that add a clickable or editable control.
const WIDGETS: &[&str] = &[
    ".button(",
    ".small_button(",
    ".checkbox(",
    "menu_button(",
    "ComboBox::",
    "TextEdit::",
    ".radio(",
    ".radio_value(",
    ".selectable_label(",
    ".selectable_value(",
    "Button::new(",
    "Slider::new(",
];

/// An `if` testing one of these reacts to an event, not to state.
const EVENTS: &[&str] = &[
    ".clicked()",
    ".changed()",
    ".hovered()",
    ".double_clicked()",
    ".secondary_clicked()",
    ".lost_focus()",
    ".gained_focus()",
    ".has_focus()",
    "key_pressed",
    "consume_key",
    ".drag_",
    ".dragged()",
    "is_pointer_button_down_on",
    "contains_pointer",
];

/// (path suffix, the whole condition with whitespace collapsed, why it stays hidden).
const ALLOWED: &[(&str, &str, &str)] = &[
    // Filled in by 3b-3d: one line per argued exception, e.g.
    (
        "help_menu.rs",
        "!crate::platform::is_store_packaged()",
        "Store and GitHub update entries replace each other by build (spec section 8)",
    ),
    // Task 3c: right-click menus and sidebars, grouped by file.
    (
        "table_view/rows.rs",
        "is_editing",
        "inline cell edit field while a cell is being edited, not a menu entry",
    ),
    (
        "table_view/rows.rs",
        "text_rect.intersects(panel_rect)",
        "inline cell edit field geometry, not a menu entry",
    ),
    (
        "table_view/rows.rs",
        "state.edit_needs_focus",
        "focus request for the inline cell edit field, not a menu entry",
    ),
    (
        "table_view/header.rs",
        "is_editing_name",
        "inline column rename text box while editing, not a menu entry",
    ),
    (
        "table_view/header.rs",
        "name_edit_rect.width() > 10.0",
        "inline column rename text box geometry, not a menu entry",
    ),
    (
        "table_view/header.rs",
        "state.edit_col_needs_focus",
        "focus request for the column rename text box, not a menu entry",
    ),
    (
        "table_view/header.rs",
        "show_search",
        "facet popup search box, an input shown only for a long value list, not a menu entry",
    ),
    (
        "table_view/header.rs",
        "state.facet_shapes_mode",
        "facet popup lists values or shapes by the mode chosen above, tab content not an entry",
    ),
    (
        "directory_tree.rs",
        "is_dir",
        "different node kind, a different menu, not a hidden entry",
    ),
    (
        "directory_tree.rs",
        "is_selected && selection_len >= 2",
        "its greyed twin (union_tree.need_two) is drawn in the else branch",
    ),
    (
        "cloud_tree.rs",
        "has_secret",
        "the sign-in method decides which one of the mutually exclusive auth controls shows, not a menu entry",
    ),
    (
        "cloud_tree.rs",
        "has_cli =>",
        "scanner: match-arm guard, not an if (sign-in button vs missing-CLI note)",
    ),
    (
        "cloud_tree.rs",
        "sign_out_armed",
        "transient inline state, not an entry",
    ),
    (
        "cloud_tree.rs",
        "entry.is_prefix",
        "different node kind, a different menu, not a hidden entry",
    ),
    (
        "cloud_tree.rs",
        "is_selected && count >= 2",
        "its greyed twin (union_tree.need_two) is drawn in the else branch",
    ),
    // Task 3d: Settings dialog.
    (
        "settings/dialog/api_section.rs",
        "self.api_form_auth.needs_secret()",
        "fields of another type stay hidden (user's choice, spec section 8)",
    ),
    (
        "settings/dialog/api_section.rs",
        "candidates.len() > 1",
        "records paths found by the last Test, a transient result, not an entry",
    ),
    (
        "settings/dialog/chat_section.rs",
        "self.chat_profile_form_kind == ChatProviderKind::OpenAi",
        "fields of another type stay hidden (user's choice, spec section 8)",
    ),
    (
        "settings/dialog/chat_section.rs",
        "matches!( self.chat_profile_form_kind, ChatProviderKind::OpenAiCompatible | ChatProviderKind::Ollama )",
        "fields of another type stay hidden (user's choice, spec section 8)",
    ),
    (
        "settings/dialog/chat_section.rs",
        "!presets.is_empty()",
        "model presets exist only for some providers, fields of another type stay hidden (user's choice, spec section 8)",
    ),
    (
        "settings/dialog/chat_section.rs",
        "provider.needs_api_key()",
        "fields of another type stay hidden (user's choice, spec section 8)",
    ),
    (
        "settings/dialog/jev_section.rs",
        "self.jev.clear_confirm",
        "transient inline state, not an entry (the same confirm row as the chat key's)",
    ),
    (
        "settings/dialog/chat_section.rs",
        "self.chat_key_clear_confirm == Some(provider)",
        "transient inline state, not an entry",
    ),
    (
        "settings/dialog/cloud_section.rs",
        "!self.cloud_form_kind.is_file_server()",
        "fields of another type stay hidden (user's choice, spec section 8)",
    ),
    (
        "settings/dialog/cloud_section.rs",
        "self.cloud_form_kind == CloudKind::S3",
        "fields of another type stay hidden (user's choice, spec section 8)",
    ),
    (
        "settings/dialog/cloud_section.rs",
        "self.cloud_form_kind == CloudKind::AzureBlob",
        "fields of another type stay hidden (user's choice, spec section 8)",
    ),
    (
        "settings/dialog/cloud_section.rs",
        "self.cloud_secret_clear_confirm.as_deref() == Some(self.cloud_form_id.as_str()) && !self.cloud_form_id.is_empty()",
        "transient inline state, not an entry",
    ),
    (
        "settings/dialog/db_section.rs",
        "self.db_form_engine.is_file()",
        "fields of another type stay hidden (user's choice, spec section 8)",
    ),
    (
        "settings/dialog/db_section.rs",
        "self.db_form_engine.polls_for_results()",
        "fields of another type stay hidden (user's choice, spec section 8)",
    ),
    (
        "settings/dialog/db_section.rs",
        "self.db_form_engine == DbEngine::Athena",
        "fields of another type stay hidden (user's choice, spec section 8)",
    ),
    (
        "settings/dialog/db_section.rs",
        "self.db_form_engine == DbEngine::Mssql",
        "fields of another type stay hidden (user's choice, spec section 8)",
    ),
    (
        "settings/dialog/db_section.rs",
        "self.db_secret_clear_confirm.as_deref() == Some(self.db_form_id.as_str())",
        "transient inline state, not an entry",
    ),
    (
        "settings/dialog/shortcuts_grid.rs",
        "self.recording == Some(action)",
        "transient inline state, not an entry",
    ),
];

fn rust_files(rel: &str) -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    if root.is_file() {
        return vec![root];
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&root).expect("scanned dir") {
        let p = entry.unwrap().path();
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        if p.extension().is_some_and(|e| e == "rs") && !name.ends_with("_tests.rs") {
            out.push(p);
        }
    }
    out.sort();
    out
}

/// The source with string, char literal and comment contents blanked to
/// spaces (newlines kept), so braces inside `"{name}"` do not count and line
/// numbers stay right.
fn blank(src: &str) -> String {
    let b = src.as_bytes();
    let mut out = b.to_vec();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    out[i] = b' ';
                    i += 1;
                }
            }
            b'"' => {
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    if b[i] == b'\\' {
                        out[i] = b' ';
                        i += 1;
                    }
                    if i < b.len() && b[i] != b'\n' {
                        out[i] = b' ';
                    }
                    i += 1;
                }
                i += 1;
            }
            b'\''
                if b.get(i + 2) == Some(&b'\'')
                    || (b.get(i + 1) == Some(&b'\\') && b.get(i + 3) == Some(&b'\'')) =>
            {
                let end = if b[i + 1] == b'\\' { i + 3 } else { i + 2 };
                for o in &mut out[i + 1..end] {
                    *o = b' ';
                }
                i = end + 1;
            }
            _ => i += 1,
        }
    }
    // Multi-byte characters inside literals are blanked byte by byte, which
    // leaves valid ASCII; outside literals they are untouched.
    String::from_utf8(out).expect("blanking keeps utf8")
}

/// Every `if` (not `if let`) with its line, condition and body.
fn ifs(code: &str) -> Vec<(usize, String, String)> {
    let b = code.as_bytes();
    let mut found = Vec::new();
    let mut i = 0;
    while let Some(off) = code[i..].find("if ") {
        let at = i + off;
        i = at + 3;
        let before = if at == 0 { b' ' } else { b[at - 1] };
        if !(before.is_ascii_whitespace() || before == b'{' || before == b'(') {
            continue;
        }
        if code[at + 3..].trim_start().starts_with("let ") {
            continue;
        }
        // Condition: up to the first `{` outside brackets.
        let (mut depth, mut j) = (0i32, at + 3);
        while j < b.len() {
            match b[j] {
                b'(' | b'[' => depth += 1,
                b')' | b']' => depth -= 1,
                b'{' if depth == 0 => break,
                b';' if depth == 0 => break,
                _ => {}
            }
            j += 1;
        }
        if j >= b.len() || b[j] != b'{' {
            continue;
        }
        let cond = code[at + 3..j].to_string();
        // Body: to the matching `}`.
        let (mut d, mut k) = (0i32, j);
        while k < b.len() {
            match b[k] {
                b'{' => d += 1,
                b'}' => {
                    d -= 1;
                    if d == 0 {
                        break;
                    }
                }
                _ => {}
            }
            k += 1;
        }
        let body = code[j + 1..k.min(b.len())].to_string();
        let line = code[..at].matches('\n').count() + 1;
        found.push((line, cond, body));
    }
    found
}

/// A condition reacting to an event, not to state. One holding `&&` is
/// state AND event (`if has_data && ui.button(..).clicked()`), which is
/// exactly the short-circuit shape, so it does not count as an event.
fn is_event(cond: &str) -> bool {
    !cond.contains("&&") && EVENTS.iter().any(|e| cond.contains(e))
}

/// `<state> && ui...`: a `&&` before the condition's first `ui` call. A `&&`
/// inside the arguments (`ui.add_enabled(a && b, ..)`) greys, it does not hide.
fn state_before_ui(cond: &str) -> bool {
    let b = cond.as_bytes();
    let ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let first_ui = (0..b.len().saturating_sub(1)).find(|&i| {
        &b[i..i + 2] == b"ui"
            && (i == 0 || !ident(b[i - 1]))
            && b.get(i + 2).is_none_or(|&c| !ident(c))
    });
    cond[..first_ui.unwrap_or(cond.len())].contains("&&")
}

fn has_widget(s: &str) -> bool {
    WIDGETS.iter().any(|w| s.contains(w))
}

/// Why this `if` hides an entry, or `None`.
fn hides(cond: &str, body: &str) -> Option<&'static str> {
    if is_event(cond) {
        None
    } else if state_before_ui(cond) && has_widget(cond) {
        Some("hidden by short-circuit")
    } else if !has_widget(cond) && has_widget(body) && !body.contains("add_enabled") {
        Some("hidden while false")
    } else {
        None
    }
}

/// Lines of `let .. = menu.<state> else { return; }`: a menu context field
/// that, when empty, ends the builder and hides every entry after it.
fn menu_let_else_returns(code: &str) -> Vec<usize> {
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(off) = code[from..].find("else {") {
        let at = from + off;
        from = at + 6;
        let start = code[..at].rfind([';', '{', '}']).map_or(0, |p| p + 1);
        let stmt = code[start..at].trim_start();
        let body = code[at + 6..].split('}').next().unwrap_or("").trim();
        if stmt.starts_with("let ") && stmt.contains("= menu.") && body.starts_with("return") {
            found.push(code[..at].matches('\n').count() + 1);
        }
    }
    found
}

/// Whitespace-collapsed condition text, the form ALLOWED holds and the
/// failure message prints.
fn collapse(cond: &str) -> String {
    cond.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn allowed(path: &Path, cond: &str) -> bool {
    // Windows walks `dialog\api_section.rs`; ALLOWED is written with `/`.
    let p = path.to_string_lossy().replace('\\', "/");
    ALLOWED
        .iter()
        .any(|(file, text, _)| p.ends_with(file) && collapse(cond) == *text)
}

#[test]
fn state_dependent_entries_are_greyed_not_hidden() {
    let mut hits = Vec::new();
    for rel in SCANNED {
        for path in rust_files(rel) {
            let src = std::fs::read_to_string(&path).unwrap();
            if src.contains("\"menu.need_table\"") {
                hits.push(format!(
                    "{}: a `menu.need_table` early return hides the whole menu",
                    path.display()
                ));
            }
            let code = blank(&src);
            for line in menu_let_else_returns(&code) {
                hits.push(format!(
                    "{}:{line}: `let .. = menu.<state> else {{ return }}` hides the entries after it",
                    path.display()
                ));
            }
            for (line, cond, body) in ifs(&code) {
                if allowed(&path, &cond) {
                    continue;
                }
                if let Some(why) = hides(&cond, &body) {
                    let short = cond.split_whitespace().collect::<Vec<_>>().join(" ");
                    hits.push(format!("{}:{line}: {why}: if {short}", path.display()));
                }
            }
        }
    }
    assert!(
        hits.is_empty(),
        "{} state-dependent entries are hidden instead of greyed (grey them with \
         octa::ui::menu_entry, or argue an exception in ALLOWED):\n{}",
        hits.len(),
        hits.join("\n")
    );
}

#[test]
fn the_scanner_catches_both_shapes_and_skips_events() {
    let code = blank(
        "if has_data && ui.button(\"x\").clicked() { a(); }\n\
         if is_db { if ui.button(\"y\").clicked() { b(); } }\n\
         if ui.button(\"z\").clicked() { c(); }\n\
         if ready { ui.add_enabled(ok, egui::Button::new(\"w\")); }\n\
         if ui.add_enabled(a && b, egui::Button::new(\"x\")).clicked() { c(); }\n\
         if a { ui.label(\"{\"); ui.button(\"x\"); }\n",
    );
    let flagged: Vec<usize> = ifs(&code)
        .iter()
        .filter(|(_, c, b)| hides(c, b).is_some())
        .map(|(l, _, _)| *l)
        .collect();
    // 1: state && click; 2: a click inside `if state`; 3 is an event; 4 is greyed.
    // 5: `&&` inside add_enabled arguments is greyed; 6: a brace in a string.
    assert_eq!(flagged, vec![1, 2, 6]);
}

#[test]
fn the_scanner_catches_a_let_else_return_on_a_menu_field() {
    let code = blank(
        "let a = 1;\nlet Some(x) = menu.tiled else {\n return;\n };\n\
         let Some(y) = other else { return; };\n",
    );
    assert_eq!(menu_let_else_returns(&code), vec![2]);
}

#[test]
fn an_allowed_condition_does_not_exempt_one_that_merely_contains_it() {
    let path = Path::new("src/ui/directory_tree.rs");
    assert!(allowed(path, "is_dir"));
    assert!(allowed(path, "is_dir\n   "));
    assert!(!allowed(path, "is_dir && ui.button(\"x\").clicked()"));
    assert!(!allowed(Path::new("src/ui/other.rs"), "is_dir"));
}
