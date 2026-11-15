import { useEffect, useState } from "react";

import { useCatalogStore } from "../store";
import { HeroBanner } from "./HeroBanner";
import { PosterRow } from "./PosterRow";

const COLUMNS = 10;

export function CatalogScreen() {
  const items = useCatalogStore((state) => state.items);
  const loading = useCatalogStore((state) => state.loading);
  const error = useCatalogStore((state) => state.error);
  const load = useCatalogStore((state) => state.load);

  const [focus, setFocus] = useState({ row: 0, col: 0 });

  useEffect(() => {
    void load();
  }, [load]);

  const rowCount = Math.ceil(items.length / COLUMNS);

  const columnCount = (row: number) =>
    Math.min(COLUMNS, Math.max(0, items.length - row * COLUMNS));

  useEffect(() => {
    if (rowCount === 0) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "ArrowLeft") {
        event.preventDefault();
        setFocus((prev) => ({ ...prev, col: Math.max(0, prev.col - 1) }));
      } else if (event.key === "ArrowRight") {
        event.preventDefault();
        setFocus((prev) => ({ ...prev, col: Math.min(columnCount(prev.row), prev.col + 1) }));
      } else if (event.key === "ArrowUp") {
        event.preventDefault();
        setFocus((prev) => {
          const nextRow = Math.max(0, prev.row - 1);
          return { row: nextRow, col: Math.min(prev.col, columnCount(nextRow)) };
        });
      } else if (event.key === "ArrowDown") {
        event.preventDefault();
        setFocus((prev) => {
          const nextRow = Math.min(rowCount - 1, prev.row + 1);
          return { row: nextRow, col: Math.min(prev.col, columnCount(nextRow)) };
        });
      } else if (event.key === "Enter") {
        // Экран деталей (#023) ещё не готов — клик по карточке пока никуда не ведёт.
        void items[focus.row * COLUMNS + focus.col];
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [items, rowCount, focus]);

  if (loading && items.length === 0) {
    return (
      <div className="flex h-full items-center justify-center">
        <p className="text-card text-text-2">Загрузка…</p>
      </div>
    );
  }

  if (error && items.length === 0) {
    return (
      <div className="flex h-full items-center justify-center">
        <p role="alert" className="text-card text-accent">
          {error}
        </p>
      </div>
    );
  }

  const rows: { start: number; end: number }[] = [];
  for (let row = 0; row < rowCount; row++) {
    rows.push({ start: row * COLUMNS, end: Math.min(items.length, (row + 1) * COLUMNS) });
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-section overflow-y-auto px-page py-section">
      {items[0] && <HeroBanner item={items[0]} />}
      {rows.map(({ start, end }, row) => (
        <div
          key={row}
          className={`transition-transform duration-200 ${
            focus.row === 0 && row > 0 ? "translate-y-4" : "translate-y-0"
          }`}
        >
          <PosterRow
            items={items.slice(start, end)}
            focusedIndex={focus.row === row ? focus.col : null}
            onSelect={() => undefined}
          />
        </div>
      ))}
    </div>
  );
}
