use super::*;

#[test]
fn the_flag_is_set_even_when_the_worker_panics() {
    let flag = Arc::new(AtomicBool::new(false));
    let moved = Arc::clone(&flag);
    let handle = std::thread::spawn(move || {
        let _guard = FlagOnDrop::new(moved, true);
        panic!("worker exploded");
    });
    assert!(handle.join().is_err());
    assert!(flag.load(Ordering::Relaxed), "guard did not run");
}
