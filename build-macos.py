#!/usr/bin/env python3
"""Build a self-contained, ad-hoc signed BlueTunes.app for the current Mac."""
import argparse
import os
from pathlib import Path
import platform
import plistlib
import re
import shutil
import subprocess
import sys
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, help="App path (default: dist/BlueTunes.app)")
    args = parser.parse_args()
    if sys.platform != "darwin":
        parser.error("Run this builder on macOS with Xcode command-line tools installed.")
    root = Path(__file__).resolve().parent
    app = (args.output or root / "dist/BlueTunes.app").expanduser().resolve()
    if app.suffix != ".app":
        parser.error("--output must end in .app")
    env = os.environ.copy()
    local_cargo = root / ".tools/cargo/bin/cargo"
    if local_cargo.is_file():
        env["CARGO_HOME"] = str(root / ".tools/cargo")
        env["RUSTUP_HOME"] = str(root / ".tools/rustup")
        env["PATH"] = str(local_cargo.parent) + os.pathsep + env.get("PATH", "")
    cargo = shutil.which("cargo", path=env.get("PATH"))
    if not cargo:
        parser.error("Install stable Rust from https://rustup.rs, then run this command again.")
    target = {"arm64": "aarch64-apple-darwin", "x86_64": "x86_64-apple-darwin"}.get(platform.machine())
    if not target:
        parser.error("Unsupported Mac architecture: " + platform.machine())
    subprocess.run([cargo, "build", "--release", "--locked", "--target", target,
                    "--target-dir", str(root / "target")], cwd=root, env=env, check=True)
    version = re.search(r'^version\s*=\s*"([^"]+)"',
                        (root / "Cargo.toml").read_text(), re.MULTILINE).group(1)
    app.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="bluetunes-") as temporary:
        staging = Path(temporary) / "BlueTunes.app"
        contents = staging / "Contents"
        executable = contents / "MacOS/bluetunes"
        resources = contents / "Resources"
        executable.parent.mkdir(parents=True)
        resources.mkdir()
        shutil.copy2(root / "target" / target / "release/bluetunes", executable)
        executable.chmod(0o755)
        iconset = Path(temporary) / "BlueTunes.iconset"
        iconset.mkdir()
        for size in (16, 32, 128, 256, 512):
            for scale in (1, 2):
                name = "icon_{0}x{0}{1}.png".format(size, "@2x" if scale == 2 else "")
                subprocess.run(["/usr/bin/sips", "-z", str(size * scale), str(size * scale),
                                str(root / "assets/bluetunes.png"), "--out", str(iconset / name)],
                               check=True, stdout=subprocess.DEVNULL)
        subprocess.run(["/usr/bin/iconutil", "-c", "icns", str(iconset), "-o",
                        str(resources / "BlueTunes.icns")], check=True)
        with (contents / "Info.plist").open("wb") as handle:
            plistlib.dump({
                "CFBundleName": "BlueTunes",
                "CFBundleDisplayName": "BlueTunes",
                "CFBundleIdentifier": "com.bluetunes.Player",
                "CFBundleExecutable": "bluetunes",
                "CFBundlePackageType": "APPL",
                "CFBundleShortVersionString": version,
                "CFBundleVersion": version,
                "CFBundleIconFile": "BlueTunes.icns",
                "NSHighResolutionCapable": True,
                "LSApplicationCategoryType": "public.app-category.music",
            }, handle)
        subprocess.run(["/usr/bin/codesign", "--force", "--sign", "-", str(staging)], check=True)
        subprocess.run(["/usr/bin/codesign", "--verify", "--strict", str(staging)], check=True)
        # Replace only files owned by this builder, preserving unrelated content.
        shutil.copytree(staging, app, dirs_exist_ok=True)
    subprocess.run(["/usr/bin/codesign", "--verify", "--strict", str(app)], check=True)
    print("Built " + str(app))
    print("Open it in Finder, or run: open " + repr(str(app)))


if __name__ == "__main__":
    main()
