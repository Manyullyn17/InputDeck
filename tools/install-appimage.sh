#!/usr/bin/env sh
# Installs the built InputDeck AppImage for the current user, with an app
# menu entry and icon. Build it first:
#   cd app && npm run tauri build -- --bundles appimage
# Run again after rebuilding to update; pass --uninstall to remove.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
id=io.github.manyullyn17.inputdeck
bin="$HOME/Applications/InputDeck.AppImage"
desktop="${XDG_DATA_HOME:-$HOME/.local/share}/applications/$id.desktop"
icon="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/256x256/apps/$id.png"

refresh() {
    update-desktop-database "$(dirname "$desktop")" 2>/dev/null || true
    gtk-update-icon-cache -q "${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor" 2>/dev/null || true
}

if [ "${1:-}" = "--uninstall" ]; then
    rm -f "$bin" "$desktop" "$icon"
    refresh
    echo "Removed InputDeck."
    exit 0
fi

# Newest AppImage the build produced.
src=$(ls -t "$root"/target/release/bundle/appimage/InputDeck_*.AppImage 2>/dev/null | head -n 1)
if [ -z "$src" ]; then
    echo "No AppImage found; build it first (see the top of this script)." >&2
    exit 1
fi

mkdir -p "$(dirname "$bin")" "$(dirname "$desktop")" "$(dirname "$icon")"
cp "$src" "$bin.tmp" && chmod +x "$bin.tmp" && mv "$bin.tmp" "$bin"
cp "$root/app/src-tauri/icons/128x128@2x.png" "$icon"

# Named after the app id so Wayland compositors match the window to it;
# StartupWMClass covers X11.
cat > "$desktop" <<EOF
[Desktop Entry]
Type=Application
Name=InputDeck
Comment=Configurator for the InputDeck macropad firmware
Exec="$bin"
Icon=$id
Categories=Utility;
StartupWMClass=inputdeck
Terminal=false
EOF

refresh
echo "Installed $(basename "$src") to $bin"
