#!/usr/bin/env bash
# Per-user install of Quiz Rust (binary, launcher entry, icons). Idempotent.
# Usage: install.sh [path/to/quiz-app-binary]    install.sh --uninstall
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
icons="$here/../icon"
bin_dir="$HOME/.local/bin"
apps_dir="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
icon_root="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor"
sizes=(16 24 32 48 64 128 256 512)

refresh_caches() {
  command -v update-desktop-database >/dev/null && update-desktop-database "$apps_dir" 2>/dev/null || true
  command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$icon_root" 2>/dev/null || true
}

if [ "${1:-}" = "--uninstall" ]; then
  rm -f "$bin_dir/quiz-rust" "$apps_dir/quiz-rust.desktop"
  for s in "${sizes[@]}"; do rm -f "$icon_root/${s}x${s}/apps/quiz-rust.png"; done
  refresh_caches
  echo "Quiz Rust uninstalled."
  exit 0
fi

binary="${1:-$here/../../../../target/release/quiz-app}"
[ -x "$binary" ] || { echo "Binary not found: $binary (build with: cargo build -p quiz-app --release)" >&2; exit 1; }

mkdir -p "$bin_dir" "$apps_dir"
install -m 755 "$binary" "$bin_dir/quiz-rust"
install -m 644 "$here/quiz-rust.desktop" "$apps_dir/quiz-rust.desktop"
for s in "${sizes[@]}"; do
  mkdir -p "$icon_root/${s}x${s}/apps"
  install -m 644 "$icons/quiz-rust-$s.png" "$icon_root/${s}x${s}/apps/quiz-rust.png"
done
refresh_caches
echo "Quiz Rust installed in $bin_dir (make sure it is in your PATH)."
