---
id: 001
type: feature
status: open
model: deepseek-v4.1-flash-free
crate: workspace
created: 2026-11-13
---

## Задача

Cargo workspace из 5 крейтов (`kino-core`, `kino-api`, `kino-player`, `kino-store`, `kino-ipc`) с пустыми `lib.rs`, общими `[workspace.dependencies]`, `rust-toolchain.toml`, `rustfmt.toml`, `clippy` в CI. M1 наполняет только `kino-core` и `kino-api`; остальные крейты остаются компилируемыми скелетами.

## Ограничения

- Зависимости строго по правилу из `ARCHITECTURE.md`: `kino-api`, `kino-player`, `kino-store` зависят только от `kino-core`; `kino-core` ни от кого.
- Нужны targets `x86_64-apple-darwin` и `aarch64-apple-darwin` в `rust-toolchain.toml`.
- Для каждого крейта создать `CRATE.md` (публичный API, пока пустой).
- Не трогать `.github/workflows/release.yml`.

## Готово когда

- `cargo check --workspace` и `cargo test --workspace` зелёные
- `cargo fmt --check` и `cargo clippy --workspace -- -D warnings` без замечаний
- `cargo check --workspace --target x86_64-apple-darwin` проходит (проверка universal)

## Заметки

Риск: `rquest` тянет BoringSSL, кросс-сборка x86_64 на Apple Silicon может потребовать cmake/NASM. Проверить в этой задаче, не откладывать на релиз. Если не собирается, остановиться и спросить заказчика про замену на `reqwest` + `rustls`.
