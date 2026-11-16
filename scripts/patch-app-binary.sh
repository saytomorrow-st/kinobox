#!/usr/bin/env bash
# Патчит Rust-бинарь так, чтобы он загружал libmpv из Contents/Frameworks
# бандла: LC_LOAD_DYLIB → @rpath/libmpv.2.dylib, плюс LC_RPATH
# @executable_path/../Frameworks.
#
# Запускается ПОСЛЕ единственной сборки `cargo tauri build --bundles app`,
# на бинаре УЖЕ ВНУТРИ .app (Contents/MacOS/KinoBox), а не в cargo target dir:
# `cargo tauri build` перелинковывает Rust-бинарь при каждом вызове независимо
# от --no-bundle/--bundles (tauri-build триггерит пересборку kinobox-app на
# смену CLI-флагов), так что патч в target dir перед вторым вызовом всегда
# стирался бы повторной линковкой. Раз сборка бандла — только одна, патч
# внутри .app никто больше не перезатирает.
#
# Использование:
#   scripts/patch-app-binary.sh <путь к бинарю внутри .app>
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <binary-in-cargo-target-dir>" >&2
  exit 64
fi

BIN="$1"
MPV_DYLIB="libmpv.2.dylib"
RPATH='@executable_path/../Frameworks'

if [[ ! -f "$BIN" ]]; then
  echo "error: binary not found: $BIN" >&2
  exit 1
fi

echo "== до патча: otool -L $BIN =="
otool -L "$BIN"

# Как линковщик записал путь к libmpv: зависит от install_name самой бутылки
# (Homebrew) — обычно абсолютный /opt/homebrew/opt/mpv/lib/libmpv.2.dylib или
# @rpath/libmpv.2.dylib. Ищем по basename, чтобы не зависеть от префикса.
current="$(otool -L "$BIN" | awk '{print $1}' | grep "/$MPV_DYLIB$" | head -1 || true)"
if [[ -z "$current" ]]; then
  echo "error: binary does not reference $MPV_DYLIB — is libmpv linked in at all?" >&2
  exit 1
fi
if [[ "$current" != "@rpath/$MPV_DYLIB" ]]; then
  install_name_tool -change "$current" "@rpath/$MPV_DYLIB" "$BIN"
  echo "patched LC_LOAD_DYLIB: $current → @rpath/$MPV_DYLIB"
fi

# LC_RPATH ровно один раз: повторный -add_rpath создаёт дубликат.
if ! otool -l "$BIN" | grep -q "$RPATH"; then
  install_name_tool -add_rpath "$RPATH" "$BIN" || \
    echo "warning: -add_rpath failed (likely small headerpad); check Frameworks LC_RPATH manually" >&2
fi

echo "== после патча: otool -L $BIN =="
otool -L "$BIN"
echo "== LC_RPATH =="
otool -l "$BIN" | grep -A2 LC_RPATH || { echo "error: no LC_RPATH in binary" >&2; exit 1; }
