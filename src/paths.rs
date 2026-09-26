use std::path::PathBuf;

pub fn data_dir() -> PathBuf {
    resolve(
        std::env::var_os("BLUETUNES_DATA_DIR").map(PathBuf::from),
        std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default(),
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from),
        cfg!(target_os = "macos"),
    )
}

fn resolve(custom: Option<PathBuf>, home: PathBuf, xdg: Option<PathBuf>, macos: bool) -> PathBuf {
    custom.unwrap_or_else(|| {
        if macos {
            home.join("Library/Application Support/BlueTunes")
        } else {
            xdg.unwrap_or_else(|| home.join(".local/share")).join("bluetunes")
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mac_uses_application_support_even_with_xdg_set() {
        assert_eq!(
            resolve(None, "/Users/music".into(), Some("/xdg".into()), true),
            PathBuf::from("/Users/music/Library/Application Support/BlueTunes")
        );
    }

    #[test]
    fn linux_keeps_existing_locations() {
        assert_eq!(resolve(None, "/home/music".into(), None, false),
            PathBuf::from("/home/music/.local/share/bluetunes"));
        assert_eq!(resolve(None, "/home/music".into(), Some("/data".into()), false),
            PathBuf::from("/data/bluetunes"));
    }

    #[test]
    fn explicit_directory_overrides_both_platforms() {
        for macos in [false, true] {
            assert_eq!(resolve(Some("/test library".into()), "/home".into(), None, macos),
                PathBuf::from("/test library"));
        }
    }
}
