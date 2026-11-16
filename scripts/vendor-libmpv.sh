#!/usr/bin/env bash
# Готовит universal (arm64+x86_64) libmpv.dylib и все её Homebrew-зависимости
# к вложению в KinoBox.app — БЕЗ записи внутрь .app.
#
# Вызывается из CI (#019): .github/workflows/build-dev.yml, .github/workflows/release.yml.
# Локально не запускается — lipo/вендоринг делаются только на раннере GitHub Actions.
#
# Почему стейджинг-каталог, а не Contents/Frameworks напрямую:
#   tauri-bundler ПЕРВЫМ делом удаляет весь *.app (fs::remove_dir_all) перед
#   копированием чего-либо. Всё, что лежит внутри .app, живёт до следующей
#   сборки бандла. Стейдж в $RUNNER_TEMP не зависит от жизненного цикла .app,
#   поэтому бандл можно пересобирать сколько угодно раз: он каждый раз берёт
#   готовые файлы оттуда.
#
# Что делает скрипт:
#   1. Обходит дерево зависимостей libmpv в двух Homebrew-префиксах
#      (arm64-бутылка и x86_64-бутылка).
#   2. Для каждого имени делает `lipo -create` → universal-файл в стейдж.
#   3. Сразу в стейдже (файлы окончательные и больше не меняются) ставит
#      install_name = @rpath/<имя> и переписывает межзависимости на @rpath.
#   4. Печатает lipo -info и otool -L для логов CI.
#
# Использование:
#   scripts/vendor-libmpv.sh <arm64-префикс-mpv> <x86_64-префикс-mpv> <стейдж-каталог>
#
# Если оба префикса совпадают (один путь дважды) — single-arch режим: вместо
# lipo -create (упал бы на "same architecture specified multiple times" для
# двух идентичных thin-файлов) файлы просто копируются как есть. Так вызывает
# build-dev.yml для x86_64-only сборки (без второго arm64-раннера).
#
# Пример (см. workflows):
#   scripts/vendor-libmpv.sh /opt/homebrew/opt/mpv "$RUNNER_TEMP/libmpv-x86_64/mpv" \
#     "$RUNNER_TEMP/vendored-libmpv"
#
# Стейдж получает структуру:
#   $STAGE/libmpv.2.dylib, $STAGE/libass.9.dylib, …
# Эти пути идут в bundle.macOS.frameworks (tauri.macos.conf.json), откуда
# бандлер скопирует их в Contents/Frameworks и подпишет вместе с приложением.
#
# Требуется macOS (lipo, otool, install_name_tool) — на Linux-раннере не работает.
set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "usage: $0 <arm64-mpv-prefix> <x86_64-mpv-prefix> <staging-dir>" >&2
  exit 64
fi

ARM_PREFIX="$1"
INTEL_PREFIX="$2"
STAGE="$3"
MPV_DYLIB="libmpv.2.dylib"

for prefix in "$ARM_PREFIX" "$INTEL_PREFIX"; do
  if [[ ! -f "$prefix/lib/$MPV_DYLIB" ]]; then
    echo "error: $MPV_DYLIB not found in $prefix/lib" >&2
    exit 1
  fi
done
if ! command -v install_name_tool >/dev/null 2>&1; then
  echo "error: install_name_tool not found (macOS/Xcode CLT required)" >&2
  exit 1
fi

rm -rf "$STAGE"
mkdir -p "$STAGE"

DEP_LIST="$(mktemp)"
DEP_QUEUE="$(mktemp)"
trap 'rm -f "$DEP_LIST" "$DEP_QUEUE"' EXIT
printf '%s\n' "$MPV_DYLIB" > "$DEP_LIST"
printf '%s\n' "$MPV_DYLIB" > "$DEP_QUEUE"

# BFS от libmpv.dylib по обоим префиксам: имена всех dylib, поставленных
# Homebrew для mpv. Системные (/usr/lib, /System) пропускаются — dyld найдёт
# их сам; берём только то, что реально лежит в префиксе mpv.
while IFS= read -r name; do
  for prefix in "$ARM_PREFIX" "$INTEL_PREFIX"; do
    lib_p="$prefix/lib/$name"
    [[ -f "$lib_p" ]] || continue
    while IFS= read -r dep; do
      dep_name="$(basename "$dep")"
      grep -qx "$dep_name" "$DEP_LIST" && continue
      if [[ -f "$ARM_PREFIX/lib/$dep_name" || -f "$INTEL_PREFIX/lib/$dep_name" ]]; then
        printf '%s\n' "$dep_name" >> "$DEP_LIST"
        printf '%s\n' "$dep_name" >> "$DEP_QUEUE"
      fi
    done < <(otool -L "$lib_p" | tail -n +2 | awk '{print $1}')
  done
done < "$DEP_QUEUE"

# Универсализация: lipo сливает две Homebrew-бутылки (arm64 + x86_64) в один
# Mach-O. install_name_tool -id делает библиотеку @rpath-адресуемой.
skipped=0
while IFS= read -r name; do
  arm_lib="$ARM_PREFIX/lib/$name"
  intel_lib="$INTEL_PREFIX/lib/$name"
  if [[ ! -f "$arm_lib" || ! -f "$intel_lib" ]]; then
    # Такого файла нет в одной из бутылок — обычно системный фреймворк или
    # необязательная зависимость. Не тащим, чтобы не ломать бандл.
    echo "skip: $name (нет в обеих бутылках)"
    skipped=$((skipped + 1))
    continue
  fi
  out="$STAGE/$name"
  if [[ "$ARM_PREFIX" == "$INTEL_PREFIX" ]]; then
    cp "$arm_lib" "$out"
    chmod +w "$out"
  else
    lipo -create -output "$out" "$arm_lib" "$intel_lib"
  fi
  install_name_tool -id "@rpath/$name" "$out"
done < "$DEP_LIST"

# Заменяет любую ссылку, чей basename совпадает с вендоренным файлом, на
# @rpath/<имя>. Работает и для абсолютных путей Homebrew (/opt/homebrew/lib/…),
# и для @rpath/…, и для @loader_path/…
#
# Идёт от РЕАЛЬНЫХ строк зависимостей target (`otool -L`), а не от DEP_LIST
# через реконструкцию regex — последнее хрупко (несовпадение по формату пути
# на Homebrew 4.x давало пустой "current" и тихо пропускало rewrite, из-за
# чего абсолютные /usr/local и /opt/homebrew пути оставались в собранных
# dylib). Проверка "basename зависимости есть в стейдже" достаточна: раз файл
# там лежит — он вендорен и адресуется через @rpath.
rewrite_deps() {
  local target="$1" dep dep_name
  while IFS= read -r dep; do
    dep_name="$(basename "$dep")"
    [[ -f "$STAGE/$dep_name" ]] || continue
    [[ "$dep" == "@rpath/$dep_name" ]] && continue
    install_name_tool -change "$dep" "@rpath/$dep_name" "$target"
  done < <(otool -L "$target" | tail -n +2 | awk '{print $1}')
}

# Все файлы уже в стейдже и окончательные — порядок обхода не важен.
for entry in "$STAGE"/*.dylib; do
  [[ -e "$entry" ]] || continue
  rewrite_deps "$entry"
done

# Оставляем для workflow список имён: по нему строится bundle.macOS.frameworks.
printf '%s\n' "$(cd "$STAGE" && ls ./*.dylib | sed 's#^\./##')" > "$STAGE/.manifest"

# Диагностика для логов CI: живой запуск недоступен, поэтому печатаем всё,
# что позволяет проверить корректность вендоринга постфактум.
echo "== lipo -info $STAGE/$MPV_DYLIB =="
lipo -info "$STAGE/$MPV_DYLIB"
echo "== otool -L $STAGE/$MPV_DYLIB =="
otool -L "$STAGE/$MPV_DYLIB"
echo "== абсолютные пути Homebrew в стейдже =="
if otool -L "$STAGE"/*.dylib | grep -E "/(opt/homebrew|usr/local)/"; then
  echo "ERROR: vendoring incomplete — абсолютные пути выше остались" >&2
  exit 1
fi
echo "OK: dylib в стейдже: $(ls "$STAGE"/*.dylib | wc -l | tr -d ' ') (пропущено ${skipped})"
