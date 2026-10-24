//! Whole-file snapshot rotation, shared by the hotkey writer and panic hook.
//! This byte-only layer can be tested without game assets or a graphics device.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Older snapshots kept beside `last.json`.
pub const PREVIOUS: usize = 5;

// The panic hook can run while the hotkey's writer is rotating these files.
// Keep the temporary write and the entire rename sequence in one transaction.
static WRITER: Mutex<()> = Mutex::new(());

thread_local! {
    static WRITING: Cell<bool> = const { Cell::new(false) };
}

// A panic in this writer invokes the hook before its mutex is released.
// Skip that recursive write rather than waiting on our own lock.
struct ActiveWrite;

impl ActiveWrite {
    fn enter() -> std::io::Result<Self> {
        if WRITING.with(|writing| writing.replace(true)) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "blackbox rotation is already active on this thread",
            ));
        }
        Ok(Self)
    }
}

impl Drop for ActiveWrite {
    fn drop(&mut self) {
        WRITING.with(|writing| writing.set(false));
    }
}

pub(crate) fn write(dir: &Path, bytes: &[u8]) -> std::io::Result<PathBuf> {
    let _active = ActiveWrite::enter()?;
    let _writer = WRITER.lock().unwrap_or_else(|e| e.into_inner());
    std::fs::create_dir_all(dir)?;
    let prev = |k: usize| dir.join(format!("prev{k}.json"));
    let last = dir.join("last.json");
    // The new file is whole before anything moves.
    let tmp = dir.join("last.tmp");
    std::fs::write(&tmp, bytes)?;
    for k in (1..PREVIOUS).rev() {
        if prev(k).exists() {
            std::fs::rename(prev(k), prev(k + 1))?;
        }
    }
    if last.exists() {
        std::fs::rename(&last, prev(1))?;
    }
    std::fs::rename(&tmp, &last)?;
    Ok(last)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Barrier;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            Self(std::env::temp_dir().join(format!(
                "openbf_rotation_{}_{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            )))
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn contents(dir: &Path) -> Vec<Vec<u8>> {
        let mut names: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        names.sort();
        assert!(names
            .iter()
            .all(|n| Path::new(n).extension().unwrap() == "json"));
        names
            .into_iter()
            .map(|n| std::fs::read(dir.join(n)).unwrap())
            .collect()
    }

    #[test]
    fn keeps_the_last_six_whole_files_in_order() {
        let dir = Scratch::new();
        for n in 0..8u8 {
            assert_eq!(write(&dir.0, &[n]).unwrap(), dir.0.join("last.json"));
            assert_eq!(contents(&dir.0).len(), (n as usize + 1).min(PREVIOUS + 1));
        }
        assert_eq!(std::fs::read(dir.0.join("last.json")).unwrap(), [7]);
        for n in 1..=PREVIOUS {
            assert_eq!(
                std::fs::read(dir.0.join(format!("prev{n}.json"))).unwrap(),
                [7 - n as u8]
            );
        }
    }

    #[test]
    fn two_writers_keep_both_snapshots_and_the_prior_history() {
        for _ in 0..64 {
            let dir = Scratch::new();
            let expected: Vec<_> = (0..6u8).map(|n| vec![n; 4096]).collect();
            for bytes in &expected[..4] {
                write(&dir.0, bytes).unwrap();
            }
            let ready = Barrier::new(2);
            std::thread::scope(|scope| {
                let writers: Vec<_> = expected[4..]
                    .iter()
                    .map(|bytes| {
                        let ready = &ready;
                        let dir = &dir.0;
                        scope.spawn(move || {
                            ready.wait();
                            write(dir, bytes)
                        })
                    })
                    .collect();
                for writer in writers {
                    writer.join().unwrap().expect("both writes must succeed");
                }
            });
            let mut actual = contents(&dir.0);
            actual.sort();
            assert_eq!(actual, expected, "every snapshot must survive exactly once");
        }
    }

    #[test]
    fn an_io_error_does_not_prevent_the_next_write() {
        let dir = Scratch::new();
        std::fs::write(&dir.0, b"a file, not a directory").unwrap();
        assert!(write(&dir.0, b"snapshot").is_err());
        std::fs::remove_file(&dir.0).unwrap();
        write(&dir.0, b"next snapshot").unwrap();
        assert_eq!(contents(&dir.0), [b"next snapshot".to_vec()]);
    }

    #[test]
    fn a_recursive_panic_write_does_not_wait_on_itself() {
        let dir = Scratch::new();
        let active = ActiveWrite::enter().unwrap();
        let _writer = WRITER.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(
            write(&dir.0, b"recursive").unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        drop(_writer);
        drop(active);
        write(&dir.0, b"after unwind").unwrap();
        assert_eq!(contents(&dir.0), [b"after unwind".to_vec()]);
    }
}
