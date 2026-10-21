use super::*;
use object_store::memory::InMemory;

fn seed_store() -> Arc<dyn ObjectStore> {
    let store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
    // Seed through our own provider so the test never calls object_store's
    // trait methods directly (their `put`/`get` live on an extension trait
    // that would otherwise need importing here).
    let provider = ObjectStoreProvider::new(store.clone());
    provider.put("top.csv", b"a,b\n1,2\n".to_vec()).unwrap();
    provider.put("sub/inner.txt", b"hi".to_vec()).unwrap();
    store
}

#[test]
fn lists_root_folders_and_files() {
    let provider = ObjectStoreProvider::new(seed_store());
    let mut entries = provider.list("").unwrap();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    // One folder "sub" and one file "top.csv".
    assert_eq!(entries.len(), 2);
    let folder = entries.iter().find(|e| e.is_prefix).unwrap();
    assert_eq!(folder.name, "sub");
    assert_eq!(folder.key, "sub/");
    let file = entries.iter().find(|e| !e.is_prefix).unwrap();
    assert_eq!(file.name, "top.csv");
    assert_eq!(file.size, Some(8));
    assert!(file.modified.is_some());
}

#[test]
fn list_recursive_flattens_everything() {
    let provider = ObjectStoreProvider::new(seed_store());
    provider.put("sub/deep/c.csv", b"x\n1\n".to_vec()).unwrap();
    let (entries, truncated) = provider.list_recursive("", 10).unwrap();
    assert!(!truncated);
    assert_eq!(entries.len(), 3);
    assert!(entries.iter().all(|e| !e.is_prefix));
    assert!(entries.iter().all(|e| e.size.is_some()));
    assert!(entries.iter().any(|e| e.key == "sub/deep/c.csv"));
}

#[test]
fn list_recursive_cap_truncates() {
    let provider = ObjectStoreProvider::new(seed_store());
    provider.put("sub/deep/c.csv", b"x".to_vec()).unwrap();
    let (entries, truncated) = provider.list_recursive("", 2).unwrap();
    assert!(truncated);
    assert_eq!(entries.len(), 2);
}

#[test]
fn list_recursive_scopes_to_prefix() {
    let provider = ObjectStoreProvider::new(seed_store());
    let (entries, _) = provider.list_recursive("sub", 10).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "inner.txt");
}

#[test]
fn get_roundtrips_object_bytes() {
    let provider = ObjectStoreProvider::new(seed_store());
    let bytes = provider.get("top.csv").unwrap();
    assert_eq!(bytes, b"a,b\n1,2\n");
}

#[test]
fn put_then_get_roundtrips() {
    let provider = ObjectStoreProvider::new(Arc::new(InMemory::new()));
    provider.put("x/y.json", b"{\"k\":1}".to_vec()).unwrap();
    assert_eq!(provider.get("x/y.json").unwrap(), b"{\"k\":1}");
}

#[test]
fn upload_file_sends_a_file_in_blocks() {
    let provider = ObjectStoreProvider::new(Arc::new(InMemory::new()));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("big.bin");
    // Over one block, so the last block is a short one.
    let bytes: Vec<u8> = (0..COPY_CHUNK_BYTES + 1000)
        .map(|i| (i % 251) as u8)
        .collect();
    std::fs::write(&path, &bytes).unwrap();
    let sent = upload_file(&provider, &path, "out/big.bin").unwrap();
    assert_eq!(sent, bytes.len() as u64);
    assert_eq!(provider.get("out/big.bin").unwrap(), bytes);

    // An empty file still makes an (empty) object.
    let empty = dir.path().join("empty.csv");
    std::fs::write(&empty, b"").unwrap();
    assert_eq!(upload_file(&provider, &empty, "empty.csv").unwrap(), 0);
    assert_eq!(provider.get("empty.csv").unwrap(), b"");
}
