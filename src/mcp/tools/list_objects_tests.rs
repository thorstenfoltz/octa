use super::*;
use crate::mcp::tools::ToolContext;

#[test]
fn empty_url_errors() {
    let ctx = ToolContext::for_mcp(Some(1000), 65536, false, true, Vec::new(), false, 0);
    let p = Params {
        url: "  ".into(),
        recursive: None,
    };
    assert!(run(&ctx, &p).is_err());
}

#[test]
fn empty_url_errors_recursive_too() {
    let ctx = ToolContext::for_mcp(Some(1000), 65536, false, true, Vec::new(), false, 0);
    let p = Params {
        url: "".into(),
        recursive: Some(true),
    };
    assert!(run(&ctx, &p).is_err());
}

#[test]
fn non_cloud_url_errors() {
    let ctx = ToolContext::for_mcp(Some(1000), 65536, false, true, Vec::new(), false, 0);
    let p = Params {
        url: "/local/file.csv".into(),
        recursive: None,
    };
    let err = run(&ctx, &p).unwrap_err().to_string();
    assert!(err.contains("not a cloud URL"), "{err}");
}
