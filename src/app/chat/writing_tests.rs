use super::*;

#[test]
fn the_prompt_carries_the_file_and_the_fence_rule() {
    let p = build_system_prompt("notes.md", "# Title\nbody\n", None);
    assert!(p.contains("notes.md"));
    assert!(p.contains("# Title\nbody\n"));
    assert!(p.contains(OPEN_FENCE));
    assert!(
        !p.contains("marked"),
        "no selection, no selection paragraph"
    );
}

#[test]
fn a_marked_passage_is_named_in_the_prompt() {
    let p = build_system_prompt("a.py", "x = 1\ny = 2\n", Some("y = 2"));
    assert!(p.contains("The user has marked this passage"));
    assert!(p.contains("y = 2"));
}

#[test]
fn the_proposal_is_the_text_between_the_fences() {
    let reply = format!("Shorter now.\n\n{OPEN_FENCE}\n# Title\nbody\n{CLOSE_FENCE}\nDone.");
    assert_eq!(proposal_in(&reply).as_deref(), Some("# Title\nbody"));
}

#[test]
fn backtick_blocks_inside_the_file_do_not_end_it() {
    let reply = format!("{OPEN_FENCE}\ntext\n```rust\nfn a() {{}}\n```\nmore\n{CLOSE_FENCE}");
    assert_eq!(
        proposal_in(&reply).as_deref(),
        Some("text\n```rust\nfn a() {}\n```\nmore")
    );
}

#[test]
fn the_last_block_wins() {
    let reply = format!(
        "{OPEN_FENCE}\nfirst\n{CLOSE_FENCE}\nOr rather:\n{OPEN_FENCE}\nsecond\n{CLOSE_FENCE}"
    );
    assert_eq!(proposal_in(&reply).as_deref(), Some("second"));
}

#[test]
fn a_plain_answer_proposes_nothing() {
    assert_eq!(
        proposal_in("It is a config file.\n```toml\na = 1\n```"),
        None
    );
}

#[test]
fn a_reply_cut_off_before_the_closing_fence_proposes_nothing() {
    // A reply that hit the token limit: half a file must never be applied.
    let reply = format!("{OPEN_FENCE}\nline one\nline tw");
    assert_eq!(proposal_in(&reply), None);
}

#[test]
fn an_empty_file_is_a_proposal_too() {
    let reply = format!("{OPEN_FENCE}\n{CLOSE_FENCE}");
    assert_eq!(proposal_in(&reply).as_deref(), Some(""));
}

#[test]
fn the_proposal_keeps_the_files_last_line_ending() {
    assert_eq!(with_ending_of("a\n", "b".into()), "b\n");
    assert_eq!(with_ending_of("a", "b\n".into()), "b");
    assert_eq!(with_ending_of("a\r\nb\r\n", "x\ny".into()), "x\r\ny\r\n");
    assert_eq!(with_ending_of("", "b".into()), "b");
}
