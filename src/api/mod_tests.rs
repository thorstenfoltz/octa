use super::*;

fn conn() -> ApiConnection {
    ApiConnection {
        id: "api-1".into(),
        name: "Orders".into(),
        base_url: "https://api.example.com/v1".into(),
        path: "orders".into(),
        ..Default::default()
    }
}

#[test]
fn url_joins_under_the_base() {
    let c = conn();
    assert_eq!(c.url_for(None), "https://api.example.com/v1/orders");
    assert_eq!(
        c.url_for(Some("/items")),
        "https://api.example.com/v1/items"
    );
    assert_eq!(c.url_for(Some("")), "https://api.example.com/v1");
}

/// The host is chosen once, by the human who saved the connection. A path -
/// which the CLI and the assistant can both supply - must not be able to move
/// the request somewhere else.
#[test]
fn a_path_cannot_change_the_host() {
    let c = conn();
    let url = c.url_for(Some("https://evil.example.com/steal"));
    assert!(
        url.starts_with("https://api.example.com/v1/"),
        "absolute paths are joined, not honoured: {url}"
    );
    assert!(!url.contains("evil.example.com/steal") || url.starts_with("https://api.example.com"));
}

#[test]
fn same_origin_compares_scheme_and_host() {
    let b = "https://api.example.com/v1/orders";
    assert!(same_origin(b, "https://api.example.com/v1/orders?page=2"));
    assert!(same_origin(b, "/v1/orders?page=2"), "relative stays home");
    assert!(
        !same_origin(b, "http://api.example.com/v1"),
        "scheme differs"
    );
    assert!(!same_origin(b, "https://other.example.com/v1"));
    assert!(
        !same_origin(b, "https://api.example.com.evil.net/v1"),
        "a suffix is not the same host"
    );
    assert!(
        same_origin(b, "https://user@api.example.com/v1"),
        "userinfo is not part of the origin"
    );
}

#[test]
fn auth_kind_mirrors_the_auth() {
    assert_eq!(ApiAuthKind::of(&ApiAuth::None), ApiAuthKind::None);
    assert_eq!(
        ApiAuthKind::of(&ApiAuth::HeaderKey { name: "X".into() }),
        ApiAuthKind::HeaderKey
    );
    assert!(!ApiAuthKind::None.needs_secret());
    assert!(ApiAuthKind::Basic.needs_secret());
    for k in ApiAuthKind::ALL {
        assert!(k.i18n_key().starts_with("api."));
    }
    for k in ApiPagingKind::ALL {
        assert!(k.i18n_key().starts_with("api."));
    }
}

#[test]
fn lookup_is_by_name_or_id_and_says_what_exists() {
    let cs = vec![conn()];
    assert_eq!(find_connection(&cs, "orders").unwrap().id, "api-1");
    assert_eq!(find_connection(&cs, "api-1").unwrap().name, "Orders");
    let err = find_connection(&cs, "nope").unwrap_err().to_string();
    assert!(err.contains("Orders"), "lists what exists: {err}");
}

#[test]
fn ids_are_unique_and_prefixed() {
    let a = ApiConnection::fresh_id();
    assert!(a.starts_with("api-"));
    assert_ne!(a, ApiConnection::fresh_id());
}

#[test]
fn tab_label_names_the_path() {
    let c = conn();
    assert_eq!(c.tab_label(None), "Orders orders");
    assert_eq!(c.tab_label(Some("/customers")), "Orders customers");
    let bare = ApiConnection {
        path: String::new(),
        ..c
    };
    assert_eq!(bare.tab_label(None), "Orders");
}
