use clap::CommandFactory;

use super::Cli;

const ADOC: &str = include_str!("../../docs/cli/octa.1.adoc");
const MARKDOWN: &str = include_str!("../../docs/cli/man-page.md");
const ROFF: &str = include_str!("../../pkg/linux/octa.1");

/// Whether `text` names `flag` as a whole flag, so `--api` is not satisfied by
/// `--api-path`. `dash` is how the text spells a hyphen: `-` in AsciiDoc and
/// Markdown, `\-` in the rendered roff.
fn names_flag(text: &str, flag: &str, dash: &str) -> bool {
    let needle = flag.replace('-', dash);
    text.match_indices(&needle).any(|(i, _)| {
        let rest = &text[i + needle.len()..];
        !rest.starts_with(dash)
            && !rest.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_')
    })
}

#[test]
fn names_flag_matches_whole_flags_only() {
    assert!(names_flag("*--api* _NAME_::", "--api", "-"));
    assert!(!names_flag("*--api-path* _PATH_::", "--api", "-"));
    assert!(names_flag(r"\fB\-\-api\fP", "--api", r"\-"));
    assert!(!names_flag(r"\fB\-\-api\-path\fP", "--api", r"\-"));
}

/// The man page (`docs/cli/octa.1.adoc`), its docs-site copy
/// (`docs/cli/man-page.md`) and the rendered page install.sh ships
/// (`pkg/linux/octa.1`) are all written by hand or rendered by hand, so a new
/// flag or an edit nobody re-rendered went unnoticed: 55 flags were missing
/// from the rendered page before this test.
#[test]
fn the_man_page_documents_every_flag() {
    let mut missing = Vec::new();
    for arg in Cli::command().get_arguments() {
        let Some(long) = arg.get_long() else { continue };
        if arg.is_hide_set() {
            continue;
        }
        let flag = format!("--{long}");
        for (file, text, dash) in [
            ("docs/cli/octa.1.adoc", ADOC, "-"),
            ("docs/cli/man-page.md", MARKDOWN, "-"),
            ("pkg/linux/octa.1", ROFF, r"\-"),
        ] {
            if !names_flag(text, &flag, dash) {
                missing.push(format!("{flag} in {file}"));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "{} man page entries are missing (after editing the adoc, re-render with \
         `asciidoctor -b manpage docs/cli/octa.1.adoc -o pkg/linux/octa.1`):\n{}",
        missing.len(),
        missing.join("\n")
    );
}
