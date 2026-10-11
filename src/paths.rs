use std::path::PathBuf;

pub fn data_dir() -> PathBuf {
    resolve(std::env::consts::OS, |name| {
        std::env::var_os(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    })
}

fn resolve(os: &str, get: impl Fn(&str) -> Option<PathBuf>) -> PathBuf {
    if let Some(custom) = get("BLUETUNES_DATA_DIR") {
        return custom;
    }
    if os == "windows" {
        return get("LOCALAPPDATA")
            .or_else(|| get("USERPROFILE").map(|home| home.join("AppData/Local")))
            .expect("Windows must provide LOCALAPPDATA or USERPROFILE")
            .join("BlueTunes");
    }
    let home = get("HOME").unwrap_or_default();
    if os == "macos" {
        home.join("Library/Application Support/BlueTunes")
    } else {
        get("XDG_DATA_HOME")
            .unwrap_or_else(|| home.join(".local/share"))
            .join("bluetunes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_env(os: &str, values: &[(&str, &str)]) -> PathBuf {
        resolve(os, |key| {
            values
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| PathBuf::from(value))
        })
    }

    #[test]
    fn windows_prefers_local_app_data_over_unix_variables() {
        assert_eq!(
            with_env(
                "windows",
                &[
                    ("LOCALAPPDATA", "C:/Users/Music Lover/AppData/Local"),
                    ("USERPROFILE", "C:/other"),
                    ("HOME", "/home/music"),
                    ("XDG_DATA_HOME", "/xdg")
                ]
            ),
            PathBuf::from("C:/Users/Music Lover/AppData/Local/BlueTunes")
        );
    }

    #[test]
    fn windows_falls_back_to_user_profile() {
        assert_eq!(
            with_env("windows", &[("USERPROFILE", "C:/Users/Música")]),
            PathBuf::from("C:/Users/Música/AppData/Local/BlueTunes")
        );
    }

    #[test]
    fn mac_uses_application_support_even_with_xdg_set() {
        assert_eq!(
            with_env(
                "macos",
                &[("HOME", "/Users/music"), ("XDG_DATA_HOME", "/xdg")]
            ),
            PathBuf::from("/Users/music/Library/Application Support/BlueTunes")
        );
    }

    #[test]
    fn linux_keeps_existing_locations() {
        assert_eq!(
            with_env("linux", &[("HOME", "/home/music")]),
            PathBuf::from("/home/music/.local/share/bluetunes")
        );
        assert_eq!(
            with_env(
                "linux",
                &[("HOME", "/home/music"), ("XDG_DATA_HOME", "/data")]
            ),
            PathBuf::from("/data/bluetunes")
        );
    }

    #[test]
    fn explicit_directory_overrides_all_platforms() {
        for os in ["linux", "macos", "windows"] {
            assert_eq!(
                with_env(os, &[("BLUETUNES_DATA_DIR", "/test library")]),
                PathBuf::from("/test library")
            );
        }
    }
}
