---
id: 019
type: feature
status: review
model: deepseek-v4-flash
crate: ci
created: 2026-11-15
---

## Задача

CI/релизная сборка universal `libmpv.dylib` (аrm64+x86_64 через `lipo`) и вендоринг в `.app`
(`Contents/Frameworks/`, `install_name_tool`/`@rpath`), чтобы пользователь не ставил Homebrew.
Обновить `.github/workflows/ci.yml` (compile/test зависимость, см. #017), `build-dev.yml` и
`release.yml`.

## Ограничения

- Пользователь не ставит ничего сам — `libmpv.dylib` внутри `.app`.
- `lipo`/вендоринг — только в GitHub Actions, не локально (слабая машина заказчика).
- `libmpv2` — LGPL-2.1: динамическое линкование (не статика), `NOTICE`/README должны называть
  исходники `mpv`/`libmpv2` и ссылку на лицензию, бандл не блокирует замену пользователем своей
  `libmpv.dylib` (просто подкладывать файл рядом, без встроенной проверки подписи/хэша).
- Источник бинарных сборок `mpv` под обе архитектуры для `lipo` — зафиксировать конкретный источник
  (например официальные universal-сборки проекта mpv для macOS, или собрать из Homebrew bottle
  arm64 + x86_64 через `brew fetch`/`arch -x86_64 brew install` на runner) и явно описать в workflow
  комментарием, откуда и почему.
- Не трогать `ci.yml` job `bindings`/`ui` сверх необходимого.

## Готово когда

- `build-dev.yml` (ручной `workflow_dispatch`) собирает `.app`, содержащий universal
  `libmpv.dylib`, с версиями двух архитектур слитыми через `lipo -info` в логе шага
- `release.yml` делает то же для тега `v*`
- `README.md`/`NOTICE` называют лицензию LGPL-2.1 и источник `libmpv`/`mpv`

## Заметки

Зависит от #017 (линковка должна работать в принципе). Можно делать параллельно с #018/#020, не
зависит от UI. Фактический запуск `build-dev.yml` возможен только после мержа цепочки PR в `main`
(известное ограничение, см. #015) — до тех пор проверяется по логам обычного `ci.yml`, который
собирает бинарь под обе архитектуры через `cargo check --target` (без полной сборки `.app`).

Источник бинарных `.dylib`: Homebrew-бутылки `mpv` под каждую архитектуру (arm64 на
`macos-latest`, x86_64 на отдельном job `macos-13`, передача артефактом). `mpv.io` не раздаёт
universal `libmpv.dylib`, сборка из исходников на раннере слишком долгая/хрупкая.

Вендоринг — однопроходная схема (после переработки, см. ниже), не двухпроходная:
1. `cargo tauri build --no-bundle` — получить universal-бинарь в cargo target dir (флаг `--no-bundle`
   подтверждён в официальном CLI-референсе Tauri v2).
2. `scripts/vendor-libmpv.sh` — BFS по зависимостям `libmpv` в обоих Homebrew-префиксах, `lipo
   -create` каждого файла в стейджинг-каталог СНАРУЖИ `.app` (`$RUNNER_TEMP/vendored-libmpv`),
   install_name → `@rpath/<имя>` сразу в стейджинге.
3. `scripts/patch-app-binary.sh` — патчит `LC_LOAD_DYLIB`/`LC_RPATH` бинаря НАПРЯМУЮ в cargo target
   dir (не внутри `.app`).
4. Генерация `src-tauri/tauri.macos.conf.json` с `bundle.macOS.frameworks`, указывающим на файлы в
   стейджинге (список берётся из `.manifest`, который пишет `vendor-libmpv.sh`).
5. Один финальный `cargo tauri build --bundles app` (build-dev.yml) или один вызов
   `tauri-action@v0` (release.yml) — копирует уже пропатченный бинарь и framework-dylib в свежий
   `.app`.

Почему не двухпроходная схема (как было в первой версии): `tauri-bundler` удаляет весь `.app`
(`fs::remove_dir_all`) перед каждой пересборкой бандла — любой патч, сделанный ВНУТРИ `.app`
(дилиб или бинарь), гарантированно стирается на следующем вызове `cargo tauri build`. Патчить
нужно источники (cargo target dir, стейджинг-каталог вне `.app`), которые бандлер каждый раз
копирует заново. Обнаружено и исправлено в ревью перед коммитом.

`bundle.macOS.frameworks` — подтверждённое реальное поле `MacConfig` в `tauri-utils` (проверено по
исходникам `tauri-bundler/src/bundle/macos/app.rs`, `copy_frameworks_to_bundle` явно поддерживает
пути `.dylib`, не только `.framework`); `tauri.macos.conf.json` — задокументированный механизм
platform-specific config (JSON Merge Patch, см. https://v2.tauri.app/develop/configuration-files/).

YAML (`build-dev.yml`, `release.yml`, `ci.yml`) и оба shell-скрипта синтаксически проверены
(`python3 -c "import yaml; ..."`, `bash -n`) — все проходят. Диагностика `otool -L`/`lipo -info` в
обоих workflow — на финальном собранном `.app` (после единственной сборки бандла), проверяет
отсутствие абсолютных путей Homebrew.
