use super::*;
use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::decrypt::{OpenSecret, open_zip_entry, zip_needs_passphrase};
use std::time::Duration;

const PASS: &str = "correct horse battery staple";

fn plain(dir: &Path, name: &str, body: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, body).unwrap();
    p
}

fn secret(p: &str) -> OpenSecret {
    OpenSecret::Passphrase(p.to_string())
}

#[test]
fn an_encrypted_zip_round_trips_with_the_passphrase() {
    let dir = tempfile::tempdir().unwrap();
    let src = plain(dir.path(), "a.csv", "id,name\n1,x\n");
    let zip = dir.path().join("a.csv.zip");
    write_encrypted_zip(&zip, &[("a.csv", &src)], PASS).unwrap();
    assert!(
        zip_needs_passphrase(&zip).unwrap(),
        "the entry must be encrypted"
    );
    let back = open_zip_entry(&zip, "a.csv", &secret(PASS)).unwrap();
    assert_eq!(back, b"id,name\n1,x\n");
}

#[test]
fn a_wrong_passphrase_is_refused_without_naming_either() {
    let dir = tempfile::tempdir().unwrap();
    let src = plain(dir.path(), "a.csv", "id\n1\n");
    let zip = dir.path().join("a.csv.zip");
    write_encrypted_zip(&zip, &[("a.csv", &src)], PASS).unwrap();
    let err = open_zip_entry(&zip, "a.csv", &secret("wrong guess")).unwrap_err();
    let msg = format!("{err:#}");
    assert!(!msg.contains(PASS) && !msg.contains("wrong guess"), "{msg}");
}

#[test]
fn an_empty_passphrase_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let src = plain(dir.path(), "a.csv", "id\n1\n");
    assert!(write_encrypted_zip(&dir.path().join("x.zip"), &[("a.csv", &src)], "").is_err());
}

#[test]
fn replacing_one_entry_keeps_the_others_encrypted_and_intact() {
    let dir = tempfile::tempdir().unwrap();
    let a = plain(dir.path(), "a.csv", "id\n1\n");
    let b = plain(dir.path(), "b.csv", "id\n2\n");
    let zip = dir.path().join("both.zip");
    write_encrypted_zip(&zip, &[("a.csv", &a), ("b.csv", &b)], PASS).unwrap();

    let a2 = plain(dir.path(), "a2.csv", "id\n1\n3\n");
    replace_entry_encrypted(&zip, "a.csv", &a2, PASS).unwrap();

    assert_eq!(
        open_zip_entry(&zip, "a.csv", &secret(PASS)).unwrap(),
        b"id\n1\n3\n"
    );
    assert_eq!(
        open_zip_entry(&zip, "b.csv", &secret(PASS)).unwrap(),
        b"id\n2\n"
    );
    let file = std::fs::File::open(&zip).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    for i in 0..archive.len() {
        assert!(
            archive.by_index_raw(i).unwrap().encrypted(),
            "entry {i} lost its encryption"
        );
    }
    assert_eq!(archive.len(), 2, "no duplicate entry");
}

#[test]
fn a_table_is_written_inside_the_zip_in_its_named_format() {
    let dir = tempfile::tempdir().unwrap();
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: "city".into(),
        data_type: "Utf8".into(),
    }];
    t.rows = vec![vec![CellValue::String("Tokyo".into())]];
    let zip = dir.path().join("report.csv.zip");
    write_table_encrypted(&t, &zip, "report.csv", PASS, &Default::default()).unwrap();
    let text =
        String::from_utf8(open_zip_entry(&zip, "report.csv", &secret(PASS)).unwrap()).unwrap();
    assert!(text.starts_with("city"), "{text}");
    assert!(text.contains("Tokyo"), "{text}");
}

#[test]
fn the_zip_name_appends_zip() {
    assert_eq!(
        zip_path_for(Path::new("/x/report.csv")),
        PathBuf::from("/x/report.csv.zip")
    );
}

#[test]
fn a_protected_temp_dir_is_removed_on_drop() {
    let root = tempfile::tempdir().unwrap();
    let tmp = ProtectedTempDir::new_in(root.path()).unwrap();
    let path = tmp.path().to_path_buf();
    assert!(
        path.file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(PROTECTED_DIR_PREFIX)
    );
    drop(tmp);
    assert!(!path.exists());
}

#[test]
fn the_sweep_removes_dead_sessions_and_keeps_live_ones() {
    let root = tempfile::tempdir().unwrap();
    let live = ProtectedTempDir::new_in(root.path()).unwrap();
    // A crashed session: the lock file exists but nobody holds it.
    let dead = root.path().join(format!("{PROTECTED_DIR_PREFIX}dead"));
    std::fs::create_dir(&dead).unwrap();
    std::fs::write(dead.join(".lock"), b"").unwrap();
    // A session that died before creating its lock file.
    let half = root.path().join(format!("{PROTECTED_DIR_PREFIX}half"));
    std::fs::create_dir(&half).unwrap();
    // Someone else's folder: never touched.
    let other = root.path().join("unrelated");
    std::fs::create_dir(&other).unwrap();

    assert_eq!(sweep_older_than(root.path(), Duration::ZERO), 2);
    assert!(live.path().exists(), "a running session's folder must stay");
    assert!(!dead.exists() && !half.exists());
    assert!(other.exists());
}

#[test]
fn the_sweep_skips_a_folder_that_was_just_created() {
    let root = tempfile::tempdir().unwrap();
    let fresh = root.path().join(format!("{PROTECTED_DIR_PREFIX}fresh"));
    std::fs::create_dir(&fresh).unwrap();
    assert_eq!(sweep_stale_protected_dirs(root.path()), 0);
    assert!(fresh.exists());
}

#[cfg(unix)]
#[test]
fn the_sweep_ignores_symlinks_and_never_blocks_on_a_fifo_lock() {
    let root = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let link = root.path().join(format!("{PROTECTED_DIR_PREFIX}link"));
    std::os::unix::fs::symlink(target.path(), &link).unwrap();
    let fifo_dir = root.path().join(format!("{PROTECTED_DIR_PREFIX}fifo"));
    std::fs::create_dir(&fifo_dir).unwrap();
    let status = std::process::Command::new("mkfifo")
        .arg(fifo_dir.join(".lock"))
        .status()
        .unwrap();
    assert!(status.success());
    let (tx, rx) = std::sync::mpsc::channel();
    let r = root.path().to_path_buf();
    std::thread::spawn(move || tx.send(sweep_older_than(&r, Duration::ZERO)).unwrap());
    let n = rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("the sweep hung");
    assert_eq!(n, 0);
    assert!(link.exists() && target.path().exists() && fifo_dir.exists());
}

#[cfg(unix)]
#[test]
fn the_private_folder_is_owner_only_and_a_replaced_zip_keeps_its_mode() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let tmp = ProtectedTempDir::new_in(root.path()).unwrap();
    let mode = std::fs::metadata(tmp.path()).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o700);

    let a = plain(root.path(), "a.csv", "id\n1\n");
    let zip = root.path().join("a.zip");
    write_encrypted_zip(&zip, &[("a.csv", &a)], PASS).unwrap();
    std::fs::set_permissions(&zip, std::fs::Permissions::from_mode(0o644)).unwrap();
    replace_entry_encrypted(&zip, "a.csv", &a, PASS).unwrap();
    let mode = std::fs::metadata(&zip).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o644);
}

#[test]
fn replace_refuses_an_empty_passphrase() {
    let dir = tempfile::tempdir().unwrap();
    let a = plain(dir.path(), "a.csv", "id\n1\n");
    let zip = dir.path().join("a.zip");
    write_encrypted_zip(&zip, &[("a.csv", &a)], PASS).unwrap();
    assert!(replace_entry_encrypted(&zip, "a.csv", &a, "").is_err());
}

#[test]
fn bytes_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let zip = dir.path().join("n.txt.zip");
    write_bytes_encrypted(b"as typed\r\n", &zip, "n.txt", PASS).unwrap();
    assert_eq!(
        open_zip_entry(&zip, "n.txt", &secret(PASS)).unwrap(),
        b"as typed\r\n"
    );
}

#[test]
fn a_failed_replace_leaves_the_old_zip_untouched_and_errors_hide_the_passphrase() {
    let dir = tempfile::tempdir().unwrap();
    let a = plain(dir.path(), "a.csv", "id\n1\n");
    let zip = dir.path().join("a.zip");
    write_encrypted_zip(&zip, &[("a.csv", &a)], PASS).unwrap();
    let before = std::fs::read(&zip).unwrap();
    let pw = "distinctive-pass-9137";
    let err = replace_entry_encrypted(&zip, "a.csv", &dir.path().join("missing"), pw).unwrap_err();
    assert!(!format!("{err:#}").contains(pw));
    assert_eq!(std::fs::read(&zip).unwrap(), before);
    let err = write_bytes_encrypted(b"x", &zip, "../evil.txt", pw).unwrap_err();
    let msg = format!("{err:#}");
    assert!(!msg.contains(pw) && msg.contains("evil"), "{msg}");
}
