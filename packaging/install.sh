#!/bin/sh
set -eu

here=$(cd "$(dirname "$0")" && pwd)
bin="${XDG_BIN_HOME:-$HOME/.local/bin}"
data="${XDG_DATA_HOME:-$HOME/.local/share}"

install -Dm755 "$here/meskd-keys" "$bin/meskd-keys"
install -Dm644 "$here/meskd-keys.png" "$data/icons/hicolor/256x256/apps/meskd-keys.png"
sed "s|^Exec=.*|Exec=$bin/meskd-keys|" "$here/meskd-keys.desktop" > "$here/.desktop.tmp"
install -Dm644 "$here/.desktop.tmp" "$data/applications/meskd-keys.desktop"
rm -f "$here/.desktop.tmp"

command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$data/applications" || true

echo "Installed to $bin/meskd-keys"
case ":$PATH:" in
  *":$bin:"*) ;;
  *) echo "Add $bin to your PATH to start it from a terminal." ;;
esac
