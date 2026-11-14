use std::io::Write;

use super::*;

fn file(content: &str) -> tempfile::NamedTempFile {
    let mut f = tempfile::NamedTempFile::new().unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f.flush().unwrap();
    f
}

fn append(f: &mut tempfile::NamedTempFile, s: &str) {
    f.as_file_mut().write_all(s.as_bytes()).unwrap();
    f.flush().unwrap();
}

#[test]
fn growth_hands_over_only_the_new_complete_lines() {
    let mut f = file("a,b\n1,2\n");
    let mut st = FollowState::start(f.path()).unwrap();
    assert_eq!(st.step(f.path(), 1 << 20).unwrap(), FollowStep::Unchanged);
    append(&mut f, "3,4\n5,6\n");
    assert_eq!(
        st.step(f.path(), 1 << 20).unwrap(),
        FollowStep::Appended("3,4\n5,6\n".into())
    );
    assert_eq!(st.step(f.path(), 1 << 20).unwrap(), FollowStep::Unchanged);
}

#[test]
fn a_half_written_line_waits_for_its_end() {
    let mut f = file("a\n");
    let mut st = FollowState::start(f.path()).unwrap();
    append(&mut f, "x");
    assert_eq!(st.step(f.path(), 1 << 20).unwrap(), FollowStep::Unchanged);
    append(&mut f, "yz\nnext");
    assert_eq!(
        st.step(f.path(), 1 << 20).unwrap(),
        FollowStep::Appended("xyz\n".into())
    );
}

#[test]
fn a_shrunk_file_reloads() {
    let f = file("a\nb\nc\n");
    let mut st = FollowState::start(f.path()).unwrap();
    std::fs::write(f.path(), "a\n").unwrap();
    assert_eq!(st.step(f.path(), 1 << 20).unwrap(), FollowStep::Reload);
}

#[test]
fn a_replaced_file_reloads_even_when_it_grew() {
    let f = file("a\nb\n");
    let mut st = FollowState::start(f.path()).unwrap();
    std::fs::write(f.path(), "x\ny\nz\n").unwrap();
    assert_eq!(st.step(f.path(), 1 << 20).unwrap(), FollowStep::Reload);
}

#[test]
fn a_big_burst_arrives_in_slices() {
    let mut f = file("h\n");
    let mut st = FollowState::start(f.path()).unwrap();
    append(&mut f, "1\n2\n3\n");
    assert_eq!(
        st.step(f.path(), 4).unwrap(),
        FollowStep::Appended("1\n2\n".into())
    );
    assert_eq!(
        st.step(f.path(), 4).unwrap(),
        FollowStep::Appended("3\n".into())
    );
}

#[test]
fn the_first_line_drops_a_bom_and_the_line_break() {
    let f = file("\u{feff}a;b\r\n1;2\r\n");
    assert_eq!(first_line(f.path()).unwrap(), "a;b");
}
