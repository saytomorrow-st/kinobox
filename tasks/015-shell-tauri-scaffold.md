---
id: 015
type: feature
status: open
model: claude-sonnet-5-5
crate: src-tauri
created: 2026-11-14
---

## Задача

Оболочка Tauri v2 (`src-tauri/`, член workspace) и каркас UI (`ui/`: Vite, React 18, TypeScript, Zustand, Tailwind). Привязка команд `kino-ipc` к TypeScript, окно входа открывается первым и единственным до входа, главное окно появляется только после успешного входа.

## Ограничения

- `pnpm` 9, Node 22+, Universal binary (aarch64 + x86_64) обязателен для релиза.
- Сгенерированные TS-типы не править руками.
- Изменение границ крейтов (новый член workspace `src-tauri`) записать в `ARCHITECTURE.md` в том же PR.

## Готово когда

- `pnpm install && cargo tauri dev` открывает окно
- `cargo check --workspace` и обе архитектуры зелёные

## Заметки

Зависит от #014.
