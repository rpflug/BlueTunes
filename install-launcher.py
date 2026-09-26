#!/usr/bin/env python3
"""Install a per-user desktop entry pointing to this checkout."""
import os
from pathlib import Path
import shutil
import subprocess

root = Path(__file__).resolve().parent
data = Path(os.environ.get('XDG_DATA_HOME', str(Path.home() / '.local/share')))
apps = data / 'applications'
icons = data / 'icons/hicolor/scalable/apps'
apps.mkdir(parents=True, exist_ok=True)
icons.mkdir(parents=True, exist_ok=True)
shutil.copyfile(root / 'assets/bluetunes.svg', icons / 'bluetunes.svg')
# Escape both desktop-entry string parsing and Exec argument parsing.
executable = str(root / 'run.sh').replace('\\', '\\\\\\\\').replace('"', '\\\\"').replace('`', '\\\\`').replace('$', '\\\\$').replace('%', '%%')
entry = apps / 'com.bluetunes.Player.desktop'
entry.write_text(f'''[Desktop Entry]
Type=Application
Name=BlueTunes
Comment=Your MP3 and FLAC music library
Exec="{executable}"
Icon=bluetunes
Terminal=false
Categories=AudioVideo;Audio;Player;
Keywords=Music;MP3;FLAC;iTunes;
StartupNotify=true
StartupWMClass=com.bluetunes.Player
''')
for command in (['desktop-file-validate', str(entry)], ['update-desktop-database', str(apps)]):
    if shutil.which(command[0]):
        subprocess.run(command, check=True)
print(f'Installed {entry}')
