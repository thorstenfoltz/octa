//! Documentation drift guards.
//!
//! Octa documents every feature twice: in the in-app Help dialog and on the
//! mkdocs site. The in-app shortcut table is generated from the live map, so it
//! cannot drift - the site's copies are hand-written and can. These tests are
//! the cheapest thing that fails when a shortcut, a CLI action or a schema
//! export target is added without documenting it, in the same spirit as
//! `every_open_as_reader_name_resolves` and
//! `menu_ellipsis_means_something_opens`.
//!
//! They compare strings against the checked-in markdown, so they run offline
//! and need no mkdocs build.

use std::path::{Path, PathBuf};

use strum::IntoEnumIterator;

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo(rel)).unwrap_or_else(|e| panic!("reading {rel}: {e}"))
}

/// The docs are prose and use a real ellipsis where a menu label uses three
/// dots. Fold both to `...` before comparing, or every dialog-opening entry
/// reads as undocumented.
fn normalise(s: &str) -> String {
    s.replace('\u{2026}', "...")
}

#[test]
fn every_shortcut_action_is_documented() {
    let doc = normalise(&read("docs/reference/shortcuts.md"));
    let missing: Vec<&str> = octa::ui::shortcuts::ShortcutAction::iter()
        .map(|a| a.label())
        .filter(|label| !doc.contains(&normalise(label)))
        .collect();
    assert!(
        missing.is_empty(),
        "ShortcutAction labels missing from docs/reference/shortcuts.md: {missing:?}\n\
         Add a row for each, or reword the existing row to match `label()`."
    );
}

#[test]
fn every_schema_export_target_is_documented() {
    let usage = read("docs/usage/schema-export.md");
    let missing: Vec<&str> = octa::data::schema_export::SchemaTarget::ALL
        .iter()
        .map(|t| t.label())
        .filter(|label| !usage.contains(*label))
        .collect();
    assert!(
        missing.is_empty(),
        "Schema export targets missing from docs/usage/schema-export.md: {missing:?}"
    );

    // The CLI and MCP pages name each target by its `--target` value, which
    // clap derives from the `SchemaTargetArg` variant name in kebab-case.
    let cli_doc = read("docs/cli/export-schema.md");
    let mcp_doc = read("docs/mcp/tools/export_schema.md");
    for value in schema_target_arg_values() {
        let quoted = format!("`{value}`");
        assert!(
            cli_doc.contains(&quoted),
            "`--target {value}` is missing from docs/cli/export-schema.md"
        );
        assert!(
            mcp_doc.contains(&quoted),
            "target `{value}` is missing from docs/mcp/tools/export_schema.md"
        );
    }
}

#[test]
fn every_cli_action_flag_is_in_the_cli_index() {
    let index = read("docs/cli/index.md");
    let missing: Vec<String> = cli_action_flags()
        .into_iter()
        .filter(|flag| !index.contains(flag))
        .collect();
    assert!(
        missing.is_empty(),
        "CLI actions missing from the table in docs/cli/index.md: {missing:?}\n\
         Every `group = \"action\"` flag needs a row there."
    );
}

#[test]
fn the_cli_action_scan_finds_the_real_flags() {
    // Guards the two scanners below: a refactor that changes how `src/cli/mod.rs`
    // is written must not turn the tests above into silent no-ops.
    let flags = cli_action_flags();
    assert!(flags.len() > 40, "only found {} action flags", flags.len());
    for expected in ["--schema", "--head", "--convert", "--sql", "--mcp"] {
        assert!(flags.iter().any(|f| f == expected), "{expected} not found");
    }
    let values = schema_target_arg_values();
    assert_eq!(
        values.len(),
        octa::data::schema_export::SchemaTarget::ALL.len(),
        "SchemaTargetArg and SchemaTarget disagree on how many targets exist"
    );
}

/// Long flags of every field in clap's `action` group, as `--kebab-case`.
///
/// `src/cli` is private to the binary, so the source is read as text rather
/// than imported. Doc comments are skipped: the module header mentions
/// `group = "action"` while explaining how to add one.
fn cli_action_flags() -> Vec<String> {
    let src = read("src/cli/mod.rs");
    let lines: Vec<&str> = src.lines().collect();
    let mut flags = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") || !line.contains(r#"group = "action""#) {
            continue;
        }
        let field = lines[i..]
            .iter()
            .take(14)
            .find_map(|l| field_name(l))
            .unwrap_or_else(|| panic!("no `pub <field>:` after line {}", i + 1));
        flags.push(format!("--{}", field.replace('_', "-")));
    }
    flags
}

/// `--target` values, from the `SchemaTargetArg` variants clap kebab-cases.
fn schema_target_arg_values() -> Vec<String> {
    let src = read("src/cli/mod.rs");
    let body = src
        .split_once("enum SchemaTargetArg {")
        .expect("SchemaTargetArg enum")
        .1
        .split_once("\n}")
        .expect("end of SchemaTargetArg")
        .0;
    body.lines()
        .map(str::trim)
        .filter(|l| l.ends_with(',') && !l.starts_with("//") && !l.starts_with('#'))
        .map(|l| kebab(l.trim_end_matches(',')))
        .collect()
}

fn field_name(line: &str) -> Option<String> {
    let rest = line.trim_start().strip_prefix("pub ")?;
    let name = rest.split_once(':')?.0.trim();
    name.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        .then(|| name.to_string())
}

fn kebab(variant: &str) -> String {
    let mut out = String::new();
    for (i, c) in variant.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('-');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}
