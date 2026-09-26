# Testing

Run the standard suite:

```sh
cargo test --release --locked
```

For the opt-in integration tests, install FFmpeg and generate synthetic audio and cover art:

```sh
fixture_dir="$(mktemp -d)"
for format in mp3 flac; do
  ffmpeg -hide_banner -loglevel error -f lavfi -i 'sine=frequency=440:duration=2' \
    -metadata title='Test Song' -metadata artist='Test Artist' -metadata album='Test Album' \
    "$fixture_dir/test.$format"
done
ffmpeg -hide_banner -loglevel error -f lavfi -i 'color=c=0x62b8ff:s=600x600' \
  -frames:v 1 "$fixture_dir/test-cover.png"
for format in mp3 flac; do
  ffmpeg -hide_banner -loglevel error -i "$fixture_dir/test.$format" \
    -i "$fixture_dir/test-cover.png" -map 0:a -map 1:v -c copy \
    -disposition:v attached_pic "$fixture_dir/art.$format"
done
BLUETUNES_TEST_AUDIO="$fixture_dir" cargo test --release --locked -- --include-ignored
```

The audio-device test opens the default output at zero volume and checks playback, pause, seek, and completion for both formats. Fixtures contain generated tones, not copyrighted music. Remove the temporary fixture directory after testing if desired.

## macOS verification

Run the same suite on macOS; the optional audio-device test exercises CoreAudio.
The path tests check macOS Application Support storage, Linux XDG storage, and
the explicit testing-directory override without changing process environment.

Build and verify the bundle:

```sh
python3 build-macos.py
codesign --verify --strict dist/BlueTunes.app
open dist/BlueTunes.app
```

In the app, check Add files and Add music folder, playback and seeking, artwork,
the visualizer, and persistence after quitting and relaunching. Copy the app
outside the checkout and confirm it still starts. Test Intel and Apple Silicon
builds on their respective hardware before distributing both architectures.

### Port validation (2026-09-26)

On Apple Silicon, the locked release build and all 22 tests passed, including
MP3/FLAC decoding, embedded artwork, and playback/pause/seek through CoreAudio.
The bundle passed `codesign --verify --strict` and plist validation, launched
through Launch Services, and remained running. Its linked libraries are all
macOS system libraries/frameworks. After enabling computer-control permissions,
the main window rendering and opening and cancelling the native Add Files dialog
were verified. Full GUI playback/import checks remain manual. Linux and Intel
builds have not been run as part of this validation.
