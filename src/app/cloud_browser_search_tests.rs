use super::{folders_below, reveal_path, search_matches};

#[test]
fn a_recursive_listing_yields_its_folders_below_the_start_only() {
    let keys = [
        "data/fact_hotel_price/part-0.parquet",
        "data/fact_hotel_price/_delta_log/0.json",
        "data/x.csv",
    ];
    assert_eq!(
        folders_below("data/", keys.into_iter()),
        [
            "data/fact_hotel_price/",
            "data/fact_hotel_price/_delta_log/"
        ]
    );
    // A connection prefix without its trailing slash is the same floor.
    assert_eq!(folders_below("data", keys.into_iter()).len(), 2);
    assert_eq!(folders_below("", ["a/b/c"].into_iter()), ["a/", "a/b/"]);
}

#[test]
fn a_slash_in_the_query_matches_the_path_instead_of_the_name() {
    assert!(search_matches(
        "part-0.parquet",
        "2024/sales/part-0.parquet",
        "4/sal"
    ));
    assert!(!search_matches(
        "part-0.parquet",
        "2024/sales/part-0.parquet",
        "sales"
    ));
    assert!(search_matches("sales", "2024/sales/", "sales"));
}

#[test]
fn revealing_a_folder_expands_every_level_down_to_it() {
    assert_eq!(
        reveal_path("", "bucket/a/b/"),
        ["", "bucket/", "bucket/a/", "bucket/a/b/"]
    );
    assert_eq!(reveal_path("data/", "data/a/"), ["data/", "data/a/"]);
}
