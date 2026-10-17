use super::bind_bucket;
use octa::cloud::{CloudConnection, CloudKind};

fn account_level(kind: CloudKind) -> CloudConnection {
    let mut c = CloudConnection::ephemeral_s3("");
    c.kind = kind;
    c.bucket = String::new();
    c.account_level = true;
    c
}

#[test]
fn account_level_keys_carry_the_bucket() {
    // The tree qualifies an account-level connection's keys as
    // `<bucket>/<key>`, and its `bucket` field is empty. Anything that
    // builds a provider from one has to split that back out first, or it
    // addresses bucket "" with a key that still names the bucket.
    let conn = account_level(CloudKind::Gcs);
    let (bound, key) = bind_bucket(&conn, "my-bucket/data/file.parquet");
    assert_eq!(bound.bucket, "my-bucket");
    assert!(!bound.account_level, "bound connection is bucket-scoped");
    assert_eq!(key, "data/file.parquet");

    // A bucket root (no sub-key) still binds.
    let (bound, key) = bind_bucket(&conn, "my-bucket");
    assert_eq!(bound.bucket, "my-bucket");
    assert_eq!(key, "");
}

#[test]
fn two_buckets_of_one_account_level_connection_are_different_stores() {
    // The copy path picks the server-side lane when both ends share a
    // bucket. Comparing the *unbound* connections would see two empty
    // buckets and wrongly take that lane across genuinely different
    // buckets.
    let conn = account_level(CloudKind::Gcs);
    let (a, _) = bind_bucket(&conn, "bucket-a/x.csv");
    let (b, _) = bind_bucket(&conn, "bucket-b/x.csv");
    assert_ne!(a.bucket, b.bucket);
    assert_eq!(conn.bucket, "", "the unbound one would have compared equal");
}

#[test]
fn a_normal_connection_passes_through() {
    let mut c = CloudConnection::ephemeral_s3("fixed-bucket");
    c.account_level = false;
    let (bound, key) = bind_bucket(&c, "data/file.csv");
    assert_eq!(bound.bucket, "fixed-bucket");
    assert_eq!(key, "data/file.csv");
}
