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
