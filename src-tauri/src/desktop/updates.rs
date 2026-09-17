//! The Android build this PC hands out to a paired phone.
//!
//! Getting a new build onto the phone otherwise means a cable and a laptop, so
//! Dun serves one over the connection the phone already trusts. The PC is not
//! a build server: it offers whatever APK has been dropped into its `updates`
//! folder, named `dun-<version>.apk`, and nothing else.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use dun_core::sync::protocol::{is_newer, UpdateOffer};

/// A package sitting in the folder, ready to be offered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Available {
    pub path: PathBuf,
    pub offer: UpdateOffer,
}

/// Remembers the hash so a 60 MB file isn't re-read on every check-in.
#[derive(Default)]
pub struct Updates {
    cached: Mutex<Option<(PathBuf, i64, u64, Available)>>,
}

impl Updates {
    /// What to offer right now, re-reading the file only when it has changed.
    pub fn current(&self, dir: &Path) -> Option<Available> {
        let (path, version) = newest(dir)?;
        let meta = std::fs::metadata(&path).ok()?;
        let size = meta.len();
        let stamp = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or_default();

        let mut cached = self.cached.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((cached_path, cached_stamp, cached_size, available)) = cached.as_ref() {
            if *cached_path == path && *cached_stamp == stamp && *cached_size == size {
                return Some(available.clone());
            }
        }

        let available = Available {
            path: path.clone(),
            offer: UpdateOffer {
                version,
                size: size as i64,
                sha256: sha256_of(&path)?,
            },
        };
        *cached = Some((path, stamp, size, available.clone()));
        Some(available)
    }
}

/// The highest-versioned `dun-<version>.apk` in the folder.
fn newest(dir: &Path) -> Option<(PathBuf, String)> {
    let mut best: Option<(PathBuf, String)> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        let Some(version) = version_of(&path) else {
            continue;
        };
        if best
            .as_ref()
            .is_none_or(|(_, current)| is_newer(&version, current))
        {
            best = Some((path, version));
        }
    }
    best
}

/// `dun-0.2.0.apk` → `0.2.0`. Anything else is somebody's stray file.
fn version_of(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    let rest = name
        .strip_suffix(".apk")
        .or_else(|| name.strip_suffix(".APK"))?;
    let version = rest
        .strip_prefix("dun-")
        .or_else(|| rest.strip_prefix("Dun-"))?;
    // Must look like a version, or a phone could be offered "latest" forever.
    is_newer(version, "0.0.0").then(|| version.to_string())
}

fn sha256_of(path: &Path) -> Option<String> {
    use sha2::{Digest, Sha256};
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).ok()?;
    use std::fmt::Write;
    let mut out = String::with_capacity(64);
    for byte in hasher.finalize() {
        let _ = write!(out, "{byte:02x}");
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, bytes: &[u8]) {
        std::fs::write(dir.join(name), bytes).unwrap();
    }

    #[test]
    fn offers_the_highest_version_and_ignores_strays() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "dun-0.9.0.apk", b"nine");
        write(dir.path(), "dun-0.10.0.apk", b"ten");
        write(dir.path(), "dun-latest.apk", b"no version here");
        write(dir.path(), "notes.txt", b"not a package");

        let updates = Updates::default();
        let available = updates.current(dir.path()).expect("one to offer");
        assert_eq!(available.offer.version, "0.10.0");
        assert_eq!(available.offer.size, 3);
        assert_eq!(
            available.offer.sha256,
            "e4432baa90819aaef51d2a7f8e148bf7e679610f3173752fabb4dcb2d0f418d3",
            "sha256 of the file it picked"
        );
    }

    #[test]
    fn an_empty_or_missing_folder_offers_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let updates = Updates::default();
        assert_eq!(updates.current(dir.path()), None);
        assert_eq!(updates.current(&dir.path().join("nowhere")), None);
    }

    #[test]
    fn a_replaced_file_is_read_again() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "dun-0.2.0.apk", b"first");
        let updates = Updates::default();
        let first = updates.current(dir.path()).unwrap().offer.sha256;

        // Same name, different build: the hash must not be the cached one.
        std::thread::sleep(std::time::Duration::from_millis(10));
        write(dir.path(), "dun-0.2.0.apk", b"second, longer");
        let second = updates.current(dir.path()).unwrap().offer.sha256;
        assert_ne!(first, second);
    }
}
