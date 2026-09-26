use eframe::egui::ColorImage;
use lofty::{file::TaggedFileExt, picture::PictureType};
use std::{io::Cursor, path::Path};

fn decode(bytes: &[u8]) -> Option<ColorImage> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let pixels = reader.decode().ok()?.thumbnail(320, 320).into_rgba8();
    Some(ColorImage::from_rgba_unmultiplied(
        [pixels.width() as usize, pixels.height() as usize],
        pixels.as_raw(),
    ))
}
pub fn load(path: &Path) -> Option<ColorImage> {
    if let Ok(file) = lofty::read_from_path(path) {
        let pictures: Vec<_> = file.tags().iter().flat_map(|t| t.pictures()).collect();
        for pic in pictures
            .iter()
            .filter(|p| p.pic_type() == PictureType::CoverFront)
            .chain(
                pictures
                    .iter()
                    .filter(|p| p.pic_type() != PictureType::CoverFront),
            )
        {
            if let Some(image) = decode(pic.data()) {
                return Some(image);
            }
        }
    }
    let folder = path.parent()?;
    for name in [
        "cover.jpg",
        "cover.png",
        "folder.jpg",
        "folder.png",
        "Cover.jpg",
        "Cover.png",
        "Folder.jpg",
        "Folder.png",
    ] {
        let path = folder.join(name);
        if std::fs::metadata(&path)
            .ok()
            .is_some_and(|m| m.len() <= 16 * 1024 * 1024)
        {
            if let Ok(bytes) = std::fs::read(path) {
                if let Some(image) = decode(&bytes) {
                    return Some(image);
                }
            }
        }
    }
    None
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Requires generated audio fixtures with embedded artwork"]
    fn embedded_mp3_and_flac_artwork() {
        let dir = std::path::PathBuf::from(
            std::env::var_os("BLUETUNES_TEST_AUDIO").expect("audio fixtures"),
        );
        for ext in ["mp3", "flac"] {
            let art = load(&dir.join(format!("art.{ext}"))).expect("embedded cover");
            assert_eq!(art.size, [320, 320]);
        }
    }
    #[test]
    fn bad_image_is_ignored() {
        assert!(decode(b"not an image").is_none());
    }
    #[test]
    fn folder_art_is_resized() {
        let dir = std::env::temp_dir().join(format!("bluetunes-art-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        image::RgbaImage::from_pixel(640, 400, image::Rgba([98, 184, 255, 255]))
            .save(dir.join("cover.png"))
            .unwrap();
        let art = load(&dir.join("nonexistent.mp3")).unwrap();
        assert_eq!(art.size, [320, 200]);
        std::fs::remove_file(dir.join("cover.png")).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
}
