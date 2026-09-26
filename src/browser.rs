use crate::Track;
use std::{collections::BTreeSet, path::PathBuf};

pub fn matches(
    track: &Track,
    filters: &[String; 3],
    depth: usize,
    query: &str,
    playlist: Option<&[PathBuf]>,
) -> bool {
    playlist.is_none_or(|paths| paths.contains(&track.path))
        && [&track.genre, &track.artist, &track.album]
            .iter()
            .zip(filters)
            .take(depth)
            .all(|(value, filter)| filter.is_empty() || *value == filter)
        && (query.is_empty()
            || format!(
                "{} {} {} {}",
                track.title, track.artist, track.album, track.genre
            )
            .to_lowercase()
            .contains(query))
}
pub fn values(
    tracks: &[Track],
    filters: &[String; 3],
    column: usize,
    query: &str,
    playlist: Option<&[PathBuf]>,
) -> BTreeSet<String> {
    tracks
        .iter()
        .filter(|t| matches(t, filters, column, query, playlist))
        .map(|t| match column {
            0 => t.genre.clone(),
            1 => t.artist.clone(),
            _ => t.album.clone(),
        })
        .collect()
}
pub fn select(filters: &mut [String; 3], column: usize, value: String) {
    if filters[column] != value {
        filters[column] = value;
        for child in &mut filters[column + 1..] {
            child.clear();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn track(genre: &str, artist: &str, album: &str) -> Track {
        Track {
            path: format!("{artist}-{album}.mp3").into(),
            title: "Song".into(),
            artist: artist.into(),
            album: album.into(),
            genre: genre.into(),
            number: 1,
            seconds: 100,
            format: "MP3".into(),
        }
    }
    #[test]
    fn genre_then_artist_always_has_songs() {
        let tracks = vec![
            track("Rock", "Arcadia", "Red"),
            track("Jazz", "Coltrane", "Blue"),
            track("Rock", "Queen", "Opera"),
        ];
        let mut filters = Default::default();
        select(&mut filters, 0, "Rock".into());
        let artists = values(&tracks, &filters, 1, "", None);
        assert_eq!(artists, BTreeSet::from(["Arcadia".into(), "Queen".into()]));
        for artist in artists {
            select(&mut filters, 1, artist);
            assert!(tracks.iter().any(|t| matches(t, &filters, 3, "", None)));
        }
    }
    #[test]
    fn changing_parent_clears_stale_children() {
        let mut f = ["Rock".into(), "Arcadia".into(), "Red".into()];
        select(&mut f, 1, "Queen".into());
        assert_eq!(f, ["Rock", "Queen", ""]);
        select(&mut f, 0, "Jazz".into());
        assert_eq!(f, ["Jazz", "", ""]);
        select(&mut f, 1, "Coltrane".into());
        select(&mut f, 2, "Blue".into());
        select(&mut f, 1, String::new());
        assert_eq!(f, ["Jazz", "", ""]);
        select(&mut f, 0, String::new());
        assert_eq!(f, ["", "", ""]);
    }
    #[test]
    fn album_choices_respect_genre_artist_search_and_playlist() {
        let tracks = vec![
            track("Rock", "Artist", "First"),
            track("Jazz", "Artist", "Second"),
            track("Rock", "Other", "Third"),
        ];
        let filters = ["Rock".into(), "Artist".into(), String::new()];
        assert_eq!(
            values(&tracks, &filters, 2, "", None),
            BTreeSet::from(["First".into()])
        );
        let paths = vec![tracks[1].path.clone()];
        assert!(values(&tracks, &filters, 2, "", Some(&paths)).is_empty());
        assert!(values(&tracks, &filters, 2, "second", None).is_empty());
    }
}
