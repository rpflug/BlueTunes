use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub volume: f32,
    pub shuffle: bool,
    pub repeat: bool,
    pub queue: Vec<PathBuf>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            volume: 0.65,
            shuffle: false,
            repeat: false,
            queue: Vec::new(),
        }
    }
}
impl Settings {
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = match fs::read(path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e.to_string()),
        };
        let mut value: Self = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        value.volume = value.volume.clamp(0.0, 1.0);
        Ok(value)
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let write = || -> Result<(), Box<dyn std::error::Error>> {
            fs::create_dir_all(path.parent().ok_or("Missing settings directory")?)?;
            if let Ok(bytes) = fs::read(path) {
                if serde_json::from_slice::<Self>(&bytes).is_err() {
                    let stamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)?
                        .as_nanos();
                    fs::copy(path, path.with_extension(format!("invalid-{stamp}.json")))?;
                }
            }
            let temp = path.with_extension("json.tmp");
            fs::write(&temp, serde_json::to_vec_pretty(self)?)?;
            fs::rename(temp, path)?;
            Ok(())
        };
        write().map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_and_clamps_old_settings() {
        let p = std::env::temp_dir().join(format!(
            "bluetunes-settings-clamp-{}.json",
            std::process::id()
        ));
        fs::write(&p, r#"{"volume":9}"#).unwrap();
        let s = Settings::load(&p).unwrap();
        assert_eq!(s.volume, 1.0);
        assert!(!s.shuffle);
        assert!(s.queue.is_empty());
        fs::remove_file(p).unwrap();
    }
    #[test]
    fn round_trip_preserves_queue_order_and_duplicates() {
        let p = std::env::temp_dir().join(format!(
            "bluetunes-settings-roundtrip-{}.json",
            std::process::id()
        ));
        let s = Settings {
            volume: 0.3,
            shuffle: true,
            repeat: true,
            queue: vec![
                "second.flac".into(),
                "first.mp3".into(),
                "second.flac".into(),
            ],
        };
        s.save(&p).unwrap();
        assert_eq!(Settings::load(&p).unwrap(), s);
        fs::remove_file(p).unwrap();
    }
}
