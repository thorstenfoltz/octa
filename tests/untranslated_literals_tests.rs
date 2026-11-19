//! English written straight into the GUI (2026-11-17): ~180 labels and status
//! lines and ~110 GUI-built errors bypassed `t()`, so a German or Japanese
//! user saw English in the middle of their language. Every one was moved into
//! the 35 catalogues; this scan keeps new ones out. It looks for a string
//! literal holding a word passed straight to a call that puts text on screen.
//! Exceptions are in ALLOWED with a reason (brand names, attribution, hidden
//! easter eggs).

use std::path::{Path, PathBuf};

const SCANNED: &[&str] = &["src/app", "src/ui", "src/view_modes", "src/main.rs"];

/// Calls whose first text argument ends up on screen. Not `Window::new`:
/// every dialog hides egui's title bar and draws its title through
/// `dialog_header`, so that text is only the window's id.
const CALLS: &[&str] = &[
    ".label(",
    ".heading(",
    ".small(",
    ".weak(",
    ".strong(",
    "RichText::new(",
    "Label::new(",
    "Button::new(",
    ".button(",
    ".small_button(",
    ".selectable_label(",
    ".checkbox(",
    ".on_hover_text(",
    ".on_disabled_hover_text(",
    ".hint_text(",
    ".selected_text(",
    "menu_button(",
    "CollapsingHeader::new(",
    ".collapsing(",
    ".hyperlink_to(",
    "ComboBox::from_label(",
    ".add_filter(",
    "status_message = Some((",
];

/// Calls taking the text second: `selectable_value(&mut v, x, "text")`.
const CALLS_TEXT_LAST: &[&str] = &[
    ".selectable_value(",
    ".radio_value(",
    ".radio(",
    ".colored_label(",
    ".toggle_value(",
    "Checkbox::new(",
    "SelectableLabel::new(",
];

/// (path suffix, the literal with its whitespace collapsed, why it stays).
/// An empty suffix matches every file.
const ALLOWED: &[(&str, &str, &str)] = &[
    // Format names, the file pickers' filter labels.
    ("", "JSON", "format name"),
    ("", "SQL", "format name"),
    ("", "PDF", "format name"),
    ("", "HTML", "format name"),
    ("", "TOML", "format name"),
    ("", "Markdown", "format name"),
    ("", "Excel", "format name"),
    ("", "DuckDB / SQLite", "format names"),
    (
        "",
        "PEM (.crt, .pem, .cer)",
        "format name and its extensions",
    ),
    // The product name and names of other products.
    ("dialogs/about.rs", "Octa", "product name"),
    ("table_view/mod.rs", "Octa", "product name"),
    ("toolbar/mod.rs", "Octa", "product name"),
    ("dialogs/about.rs", "mailto:{}", "a link, not prose"),
    (
        "dialogs/ai_report.rs",
        "{} TypeSafe AI",
        "company name after a translated verb",
    ),
    ("dialogs/sql_write_back.rs", "DuckDB", "engine name"),
    ("dialogs/sql_write_back.rs", "SQLite", "engine name"),
    (
        "view_modes/map.rs",
        "© OpenStreetMap contributors",
        "OSM's required attribution, verbatim",
    ),
    // Example values in an empty field's hint: what the user would type.
    ("dialogs/anonymize.rs", "person_id", "example column name"),
    (
        "dialogs/db_compare.rs",
        "s3://bucket/exports/day.parquet",
        "example path",
    ),
    (
        "dialogs/open_url.rs",
        "https://example.org/data.csv",
        "example URL",
    ),
    (
        "dialogs/rename_columns.rs",
        "column,new_name",
        "example CSV header the dialog reads",
    ),
    (
        "dialog/api_section.rs",
        "Accept: application/json",
        "example HTTP header",
    ),
    (
        "dialog/chat_section.rs",
        "http://localhost:11434",
        "Ollama's default address",
    ),
    (
        "dialog/cloud_section.rs",
        "files.example.com",
        "example host",
    ),
    (
        "dialog/cloud_section.rs",
        "~/.ssh/id_ed25519",
        "example key path",
    ),
    ("dialog/cloud_section.rs", "/home/anna", "example folder"),
    (
        "dialog/cloud_section.rs",
        "https://cloud.example.com/remote.php/dav/files/USER/",
        "example WebDAV URL",
    ),
    (
        "dialog/directory_tree_section.rs",
        "master",
        "example branch name",
    ),
    (
        "dialog/map_section.rs",
        "https://tile.openstreetmap.org/{z}/{x}/{y}.png",
        "example tile URL",
    ),
    (
        "dialog/opening_section.rs",
        "log4j, myproj, rawdata",
        "example file extensions",
    ),
    // Units and code.
    (
        "dialogs/raw_perf_prompt.rs",
        "\\\"{}\\\" ({:.1} MB)",
        "file name and size, MB is a unit",
    ),
    (
        "dialog/appearance_section.rs",
        "{} pt",
        "font size, pt is a unit",
    ),
    (
        "sql/workspace.rs",
        "{} `SELECT * FROM {qualified} LIMIT 100;`",
        "SQL code after a translated hint",
    ),
    (
        "sql/workspace.rs",
        "{} `SELECT * FROM {qualified} LIMIT 100`",
        "SQL code after a translated hint",
    ),
    // Hidden easter eggs: never documented, never translated.
    (
        "dialogs/about.rs",
        "\\u{1f419} Eight tentacles, eight clicks. \\ You found the kraken's lair.",
        "easter egg",
    ),
    ("easter_eggs.rs", "\\u{1f389} Konami!", "easter egg"),
    (
        "easter_eggs.rs",
        "\\u{1f308} Rainbow mode unlocked",
        "easter egg",
    ),
    ("easter_eggs.rs", "\\u{2744} Let it snow!", "easter egg"),
    (
        "status_bar.rs",
        "\\u{1f419} The kraken stirs from the depths...",
        "easter egg",
    ),
];

/// Every `.rs` file under `rel`, recursively, without the test siblings.
fn rust_files(rel: &str) -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(p) = stack.pop() {
        if p.is_dir() {
            stack.extend(std::fs::read_dir(&p).unwrap().map(|e| e.unwrap().path()));
        } else if p.extension().is_some_and(|e| e == "rs")
            && !p.to_string_lossy().ends_with("_tests.rs")
        {
            out.push(p);
        }
    }
    out.sort();
    out
}

/// The source with `//` comments blanked to spaces (newlines kept) and
/// string literals left alone, so a doc example is not a hit and a `//`
/// inside a URL literal is not a comment.
fn strip_comments(src: &str) -> String {
    let b = src.as_bytes();
    let mut out = b.to_vec();
    let ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    out[i] = b' ';
                    i += 1;
                }
            }
            b'r' if (i == 0 || !ident(b[i - 1])) && matches!(b.get(i + 1), Some(b'"' | b'#')) => {
                let hashes = b[i + 1..].iter().take_while(|&&c| c == b'#').count();
                let open = i + 1 + hashes;
                if b.get(open) != Some(&b'"') {
                    i += 1;
                    continue;
                }
                let close = format!("\"{}", "#".repeat(hashes));
                i = src[open + 1..]
                    .find(&close)
                    .map_or(b.len(), |p| open + 1 + p + close.len());
            }
            b'"' => {
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
                i += 1;
            }
            b'\''
                if b.get(i + 2) == Some(&b'\'')
                    || (b.get(i + 1) == Some(&b'\\') && b.get(i + 3) == Some(&b'\'')) =>
            {
                i += if b[i + 1] == b'\\' { 4 } else { 3 };
            }
            _ => i += 1,
        }
    }
    String::from_utf8(out).expect("only ASCII bytes were blanked")
}

/// The string literal `code` starts with (after `&`, `format!(` or a
/// `RichText::new(` wrapper), as written between its quotes.
fn leading_literal(code: &str) -> Option<&str> {
    let mut s = code.trim_start().trim_start_matches('&');
    for wrap in ["format!(", "egui::RichText::new(", "RichText::new("] {
        if let Some(rest) = s.strip_prefix(wrap) {
            s = rest.trim_start();
        }
    }
    let body = s.strip_prefix('"')?;
    let b = body.as_bytes();
    let mut j = 0;
    while j < b.len() && b[j] != b'"' {
        j += if b[j] == b'\\' { 2 } else { 1 };
    }
    Some(&body[..j.min(b.len())])
}

/// Does the literal hold a word? Placeholders (`{}`, `{name}`, `{:.2}`) and
/// escapes (`\n`, `\u{2014}`) are not words; two letters in a row are.
fn has_word(lit: &str) -> bool {
    let mut plain = String::new();
    let mut depth = 0;
    let mut chars = lit.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if chars.next() == Some('u') {
                    for c in chars.by_ref() {
                        if c == '}' {
                            break;
                        }
                    }
                }
                plain.push(' ');
            }
            '{' => depth += 1,
            '}' => depth -= 1,
            _ if depth == 0 => plain.push(c),
            _ => {}
        }
    }
    plain
        .split(|c: char| !c.is_alphabetic())
        .any(|w| w.chars().count() >= 2)
}

/// The argument list after `open` (the index just past a call's `(`), cut at
/// its closing bracket.
fn args(code: &str, open: usize) -> &str {
    let b = code.as_bytes();
    let (mut depth, mut j, mut in_str) = (1i32, open, false);
    while j < b.len() {
        match b[j] {
            b'\\' if in_str => j += 1,
            b'"' => in_str = !in_str,
            b'(' | b'[' | b'{' if !in_str => depth += 1,
            b')' | b']' | b'}' if !in_str => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
        j += 1;
    }
    &code[open..j.min(b.len())]
}

/// The last top-level argument of an argument list.
fn last_arg(args: &str) -> &str {
    let b = args.as_bytes();
    let (mut depth, mut in_str, mut start) = (0i32, false, 0);
    let mut j = 0;
    while j < b.len() {
        match b[j] {
            b'\\' if in_str => j += 1,
            b'"' => in_str = !in_str,
            b'(' | b'[' | b'{' if !in_str => depth += 1,
            b')' | b']' | b'}' if !in_str => depth -= 1,
            b',' if !in_str && depth == 0 && j + 1 < b.len() => start = j + 1,
            _ => {}
        }
        j += 1;
    }
    &args[start..]
}

/// (line, literal) of every English literal handed to a screen call.
fn hits(code: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    let mut check = |at: usize, arg: &str| {
        if let Some(lit) = leading_literal(arg).filter(|l| has_word(l)) {
            found.push((code[..at].matches('\n').count() + 1, lit.to_string()));
        }
    };
    for call in CALLS {
        for (at, _) in code.match_indices(call) {
            check(at, &code[at + call.len()..]);
        }
    }
    for call in CALLS_TEXT_LAST {
        for (at, _) in code.match_indices(call) {
            check(at, last_arg(args(code, at + call.len())));
        }
    }
    found.sort();
    found.dedup();
    found
}

fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn allowed(path: &Path, lit: &str) -> bool {
    // Windows walks `dialog\api_section.rs`; ALLOWED is written with `/`.
    let p = path.to_string_lossy().replace('\\', "/");
    ALLOWED
        .iter()
        .any(|(file, text, _)| p.ends_with(file) && collapse(lit) == *text)
}

#[test]
fn gui_text_goes_through_the_catalogues() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut found = Vec::new();
    for rel in SCANNED {
        for path in rust_files(rel) {
            let code = strip_comments(&std::fs::read_to_string(&path).unwrap());
            for (line, lit) in hits(&code) {
                if !allowed(&path, &lit) {
                    let shown = path.strip_prefix(root).unwrap_or(&path).display();
                    found.push(format!("{shown}:{line}: \"{lit}\""));
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "{} English texts are written into the GUI code (add a key to every \
         locales/*.toml and call t(), or argue an exception in ALLOWED):\n{}",
        found.len(),
        found.join("\n")
    );
}

#[test]
fn the_scanner_finds_english_and_skips_keys_and_placeholders() {
    let code = strip_comments(
        "ui.label(\"Saved\");\n\
         ui.label(t(\"msg.saved\"));\n\
         ui.label(format!(\"{} / {}\", a, b));\n\
         // ui.label(\"in a comment\");\n\
         ui.button(RichText::new(\"Close\").small());\n\
         ui.selectable_value(&mut v, Kind::A, \"Bars\");\n\
         ui.label(\"x\");\n\
         let url = \"https://a.b\"; ui.label(\"{n}\\u{2014}\");\n\
         self.status_message = Some((format!(\"Copied {n} rows\"), now));\n\
         ui.colored_label(red, \"Could not read\");\n",
    );
    let lines: Vec<usize> = hits(&code).iter().map(|(l, _)| *l).collect();
    // 1 plain, 5 wrapped in RichText (found twice, kept once), 6 and 10
    // text-last, 9 status. 2 is a key, 3 and 8 are placeholders/escapes,
    // 4 a comment, 7 a single letter.
    assert_eq!(lines, vec![1, 5, 6, 9, 10]);
}

#[test]
fn an_allowed_literal_exempts_only_itself_in_its_own_file() {
    let about = Path::new("src/app/dialogs/about.rs");
    assert!(allowed(about, "Octa"));
    assert!(!allowed(about, "Octa is great"));
    assert!(!allowed(Path::new("src/app/dialogs/other.rs"), "Octa"));
}
