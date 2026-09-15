//! Chime playback on a dedicated thread.
//!
//! The output device is opened per chime, so a chime plays on whatever device
//! is the default right now (headphones plugged in, Bluetooth connected).

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use dun_core::model::ChimeRef;
use sha2::{Digest, Sha256};

/// Bundled chimes: (id, label, bytes). The same ids exist on Android.
pub const BUNDLED: &[(&str, &str, &[u8])] = &[
    (
        "bell",
        "Bell",
        include_bytes!("../../resources/chimes/bell.wav"),
    ),
    (
        "rise",
        "Rise",
        include_bytes!("../../resources/chimes/rise.wav"),
    ),
    (
        "pulse",
        "Pulse",
        include_bytes!("../../resources/chimes/pulse.wav"),
    ),
    (
        "soft",
        "Soft",
        include_bytes!("../../resources/chimes/soft.wav"),
    ),
];

/// Extensions accepted for imported sounds.
pub const IMPORT_EXTENSIONS: &[&str] = &["wav", "mp3", "ogg"];

enum Job {
    Bytes(&'static [u8], f32),
    File(PathBuf, f32),
}

#[derive(Clone)]
pub struct Audio {
    tx: mpsc::Sender<Job>,
    sounds_dir: PathBuf,
}

impl Audio {
    pub fn start(sounds_dir: PathBuf) -> Self {
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("chime".into())
            .spawn(move || {
                for job in rx {
                    if let Err(e) = play(job) {
                        eprintln!("chime failed: {e}");
                    }
                }
            })
            .expect("spawn chime thread");
        Audio { tx, sounds_dir }
    }

    /// Plays `chime` (or `fallback` if it's missing) at `volume` (0–1).
    pub fn play(&self, chime: Option<&ChimeRef>, fallback: &ChimeRef, volume: f32) {
        let volume = volume.clamp(0.0, 1.0);
        let job = self
            .resolve(chime)
            .or_else(|| self.resolve(Some(fallback)))
            .unwrap_or(Sound::Bytes(BUNDLED[0].2));
        let _ = self.tx.send(match job {
            Sound::Bytes(b) => Job::Bytes(b, volume),
            Sound::File(p) => Job::File(p, volume),
        });
    }

    fn resolve(&self, chime: Option<&ChimeRef>) -> Option<Sound> {
        match chime? {
            ChimeRef::Bundled { id } => BUNDLED
                .iter()
                .find(|(i, _, _)| i == id)
                .map(|(_, _, b)| Sound::Bytes(b)),
            ChimeRef::Custom { sha256, .. } => {
                let path = IMPORT_EXTENSIONS
                    .iter()
                    .map(|ext| self.sounds_dir.join(format!("{sha256}.{ext}")))
                    .find(|p| p.exists())?;
                Some(Sound::File(path))
            }
        }
    }
}

enum Sound {
    Bytes(&'static [u8]),
    File(PathBuf),
}

fn play(job: Job) -> Result<(), String> {
    let mut sink = rodio::DeviceSinkBuilder::open_default_sink()
        .map_err(|e| format!("no audio output: {e}"))?;
    sink.log_on_drop(false);
    let player = rodio::Player::connect_new(sink.mixer());
    match job {
        Job::Bytes(bytes, volume) => {
            player.set_volume(volume);
            player.append(rodio::Decoder::new(Cursor::new(bytes)).map_err(|e| e.to_string())?);
        }
        Job::File(path, volume) => {
            player.set_volume(volume);
            let file =
                std::fs::File::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            player.append(rodio::Decoder::try_from(file).map_err(|e| e.to_string())?);
        }
    }
    player.sleep_until_end();
    Ok(())
}

/// Copies a user's sound file into the sounds folder under its content hash
/// after checking it decodes. Returns the chime reference to store.
pub fn import(src: &Path, sounds_dir: &Path) -> Result<ChimeRef, String> {
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|e| IMPORT_EXTENSIONS.contains(&e.as_str()))
        .ok_or("choose a .wav, .mp3 or .ogg file")?;
    let bytes = std::fs::read(src).map_err(|e| format!("{}: {e}", src.display()))?;
    if bytes.len() > 20 * 1024 * 1024 {
        return Err("sound files must be under 20 MB".into());
    }
    rodio::Decoder::new(Cursor::new(bytes.clone()))
        .map_err(|_| "that file isn't a sound Dun can play")?;

    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    std::fs::create_dir_all(sounds_dir).map_err(|e| e.to_string())?;
    let dest = sounds_dir.join(format!("{sha256}.{ext}"));
    if !dest.exists() {
        std::fs::write(&dest, &bytes).map_err(|e| format!("{}: {e}", dest.display()))?;
    }
    let name = src
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Custom sound")
        .to_string();
    Ok(ChimeRef::Custom { sha256, name })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_chimes_decode() {
        for (id, _, bytes) in BUNDLED {
            let decoder = rodio::Decoder::new(Cursor::new(*bytes));
            assert!(decoder.is_ok(), "{id} should decode");
        }
    }

    #[test]
    fn import_hashes_validates_and_dedupes() {
        let dir = std::env::temp_dir().join(format!("dun-sounds-{}", dun_core::ids::new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("My Alarm.wav");
        std::fs::write(&src, BUNDLED[1].2).unwrap();

        let sounds = dir.join("sounds");
        let chime = import(&src, &sounds).unwrap();
        let ChimeRef::Custom { sha256, name } = &chime else {
            panic!("expected custom chime")
        };
        assert_eq!(name, "My Alarm");
        assert!(sounds.join(format!("{sha256}.wav")).exists());
        assert_eq!(
            import(&src, &sounds).unwrap(),
            chime,
            "same file imports to the same reference"
        );

        let bogus = dir.join("notes.wav");
        std::fs::write(&bogus, b"definitely not audio").unwrap();
        assert!(import(&bogus, &sounds).is_err());
        assert!(import(&dir.join("x.flac"), &sounds).is_err());

        let audio = Audio::start(sounds.clone());
        assert!(matches!(audio.resolve(Some(&chime)), Some(Sound::File(_))));
        assert!(audio
            .resolve(Some(&ChimeRef::Custom {
                sha256: "missing".into(),
                name: "x".into()
            }))
            .is_none());
        std::fs::remove_dir_all(&dir).ok();
    }
}
