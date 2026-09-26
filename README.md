# BlueTunes

A native Rust music player for Linux with a classic iTunes-style table, a navy-and-cyan interface, MP3/FLAC playback, playlists, album artwork, and audio-reactive visualizations.

An independent hobby project, not affiliated with Apple.

## Run

Install a current stable Rust toolchain, a C toolchain, pkg-config, and ALSA development libraries. A Wayland or X11 desktop with OpenGL and a working desktop portal is required.

From the project directory, run:

```sh
./run.sh
```

The launcher uses a compiled release binary if available, otherwise it builds one with Cargo.

To install a launcher and icon for your user account (requires Python 3):

```sh
python3 install-launcher.py
```

Then open **BlueTunes** from your application menu. The launcher points to this checkout; keep it in place or reinstall the launcher after moving it.

## Use

- **Add music folder** scans subfolders for MP3 and FLAC files. **Add files** imports individual tracks. You can also drop files/folders into the window.
- Double-click a song to play. Space toggles play/pause while you are not typing.
- Click column headings to sort; drag column boundaries to resize.
- Filter with Genre, Artist, Album, and search; **Clear filters** resets them. Artist choices follow the selected genre, and album choices follow both. Changing a parent clears its child selections.
- Check songs in the leftmost column (or click a song title), then use **Add to playlist…** above the table. Use **Select all shown** to add an entire filtered artist or album. The menu also offers **Create and add songs** for a new playlist. Duplicate entries are skipped.
- Right-click a song to play it, enqueue it, or add it to a playlist.
- Create playlists in the sidebar; right-click within a playlist to remove a song from it.
- Shuffle chooses another song from the current filtered view. Repeat all wraps playback. Explicit queued songs play first.
- Seek with the playback slider and adjust volume at the top.
- Volume, shuffle, repeat, and the queue are restored on restart; playback does not start automatically.
- Selecting a song shows its embedded cover, or a nearby cover.jpg/cover.png/folder.jpg/folder.png. Artwork loads in the background.

Library metadata and playlists are saved to `$XDG_DATA_HOME/bluetunes/library.json` (normally `~/.local/share/bluetunes/library.json`). Audio files are referenced in place; they are never copied or edited. Set `BLUETUNES_DATA_DIR` to use a separate library for testing.

## Build / test

```sh
cargo build --release --locked
cargo test --release --locked
```

Linux build requirements: C toolchain, pkg-config, ALSA headers, and a Wayland or X11 desktop with OpenGL support. File dialogs use the desktop portal.

The default test run covers filtering, playlists, drag-and-drop, settings, and visualization. Optional audio and embedded-artwork tests require generated fixtures and a working audio device; see [testing instructions](TESTING.md).

## Current limits

No tag editing, gapless playback, media-key integration, or automatic folder watching yet. Re-importing a folder skips existing paths. Moving source files requires importing their new location.

Playback settings are stored beside the library in `settings.json`. Reinstall the desktop launcher after moving this project with `python3 install-launcher.py`.

## Visualizer

Click **Visualizer** below the playback controls to replace the library table with audio-reactive patterns. Choose **Aurora**, **Kaleidoscope**, or **Waveform**. Patterns react to decoded music level and bass; they do not use the microphone. Pause stops animation; **Back to music**, the Visualizer button, or Escape restores the table. Playback controls remain available. You can also launch directly with `./run.sh --visualizer`.

The visualizer samples the first audio channel before the volume control, so turning the volume down does not weaken the patterns. Its mode is session-only.

## Reorder a playlist

Open a playlist in the sidebar, then drag the numbered handle in the **ORDER** column. Drop in the upper or lower half of another row to insert before or after it; a cyan line marks the position. Changes save immediately. If you sorted a column, click **Playlist order** above the table to enable dragging again.

Normal playlist playback follows this saved order, even while viewing a column sort. Shuffle still randomizes playback, and explicit Up next items still take priority. Reordering a filtered playlist moves the song relative to the drop target without removing hidden songs. Drag one song at a time; checkboxes are for adding songs to playlists.
