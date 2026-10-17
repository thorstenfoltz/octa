use super::{join_path, split_path};

#[test]
fn path_roundtrip() {
    assert_eq!(split_path(""), Vec::<&str>::new());
    assert_eq!(split_path("main"), vec!["main"]);
    assert_eq!(split_path("main\u{1f}sales"), vec!["main", "sales"]);
    assert_eq!(join_path(&["main", "sales"]), "main\u{1f}sales");
    assert_eq!(join_path(&["public"]), "public");
}
