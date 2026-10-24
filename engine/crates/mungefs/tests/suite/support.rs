//! Shared fixture I/O; callers retain their own gates, sorting and reference assertions.

use mungefs::{chunks, open, Chunk};
use std::path::{Path, PathBuf};

pub(super) fn lvl(rel: &str) -> Option<Vec<u8>> {
    std::fs::read(format!(
        "{}/../../../extracted/{rel}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .ok()
}

pub(super) fn tern_of(buf: &[u8]) -> Chunk<'_> {
    let inner = open(buf).unwrap();
    chunks(inner)
        .flatten()
        .find(|c| &c.tag == b"tern")
        .expect("a tern")
}

pub(super) fn walk_lvls(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk_lvls(&p, out);
        } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("lvl")) {
            out.push(p);
        }
    }
}
