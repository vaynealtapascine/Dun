//! Fetching the newer build the PC offered.
//!
//! Nothing here happens on its own: a check-in notes what the PC is holding,
//! and this runs only when the user asks for it. The download goes over the
//! same pinned connection as everything else, and Android is handed the file
//! only once its hash matches what the PC promised.

use std::path::PathBuf;

use dun_core::sync::protocol::UpdateOffer;

use super::core;
use super::sync::{self, UPDATE_KEY};

/// The offer the last check-in heard about, if it is still worth showing.
pub fn offered(data_dir: &str) -> Option<UpdateOffer> {
    core::with_engine(data_dir, |engine| {
        engine
            .store()
            .local_get::<Option<UpdateOffer>>(UPDATE_KEY)
            .ok()
            .flatten()
            .flatten()
    })
    .ok()
    .flatten()
    .filter(|offer| dun_core::sync::protocol::is_newer(&offer.version, dun_core::VERSION))
}

/// Downloads the offered package and returns where it landed.
pub fn download(data_dir: &str) -> Result<PathBuf, String> {
    let offer = offered(data_dir).ok_or("There's no newer build on your PC")?;
    let peer = core::with_engine(data_dir, |engine| sync::paired_pc(&engine.peers()))?
        .ok_or("This phone isn't paired with a PC")?;
    let (Some(token), Some(fingerprint)) = (peer.token.clone(), peer.fingerprint.clone()) else {
        return Err("This phone isn't paired with a PC".into());
    };

    // Under `files/` so the FileProvider can name the folder without knowing
    // which Android user this is, and not in the cache, which Android may
    // clear out from under the installer.
    let dir = PathBuf::from(data_dir).join("files").join("updates");
    std::fs::create_dir_all(&dir).map_err(|e| format!("couldn't make room for it: {e}"))?;
    let into = dir.join(format!("dun-{}.apk", offer.version));

    // Already here from an attempt that stopped at the installer: 250 MB over
    // Wi-Fi is not worth repeating to arrive at the same bytes.
    if matches!(std::fs::metadata(&into), Ok(meta) if meta.len() == offer.size as u64)
        && file_sha256(&into).is_some_and(|got| got.eq_ignore_ascii_case(&offer.sha256))
    {
        return Ok(into);
    }

    let mut addrs = peer.addrs.clone();
    if let Some(last) = peer.last_ok_addr.clone() {
        addrs.retain(|a| *a != last);
        addrs.insert(0, last);
    }

    let mut last_error = "couldn't reach the PC".to_string();
    for addr in addrs {
        let result = sync::runtime().block_on(dun_sync::client::download_apk(
            &fingerprint,
            &addr,
            peer.port,
            &token,
            &offer.sha256,
            &into,
        ));
        match result {
            Ok(_) => {
                // Old downloads are dead weight on a phone.
                sweep(&dir, &into);
                return Ok(into);
            }
            Err(e) => last_error = e.to_string(),
        }
    }
    Err(last_error)
}

fn file_sha256(path: &std::path::Path) -> Option<String> {
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

/// Removes every package in the folder except the one just fetched.
fn sweep(dir: &std::path::Path, keep: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path != keep {
            let _ = std::fs::remove_file(path);
        }
    }
}
