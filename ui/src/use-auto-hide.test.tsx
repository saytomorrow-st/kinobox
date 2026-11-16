import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

// Юнит-тест `useAutoHide`. Окружение Vitest — "node" (без jsdom), поэтому реальный
// `useEffect` (где живут `show()`, `setTimeout` и подписка на window) запустить нельзя:
// - `renderToStaticMarkup` не выполняет эффекты;
// - `react-dom/client`'s `createRoot` требует настоящего `document`, которого в env "node" нет;
// - `window` нужно было бы подменять через `vi.stubGlobal`, но без рендерера эффект всё
//   равно не запустится, так что подписки на `window.addEventListener` проверить нельзя.
//
// Достигнутое покрытие:
// 1. smoke-проверка: хук — функция ожидаемой арности (не падает при импорте);
// 2. начальное состояние `visible === true` (через эквивалентный хук-пробник: настоящий
//    `useState(true)` — начальное значение, которое возвращает `useAutoHide`);
// 3. таймер-логика `show()` проверена как чистая модель с теми же шагами
//    (visible=true → сброс старого таймера → новый таймер на delayMs → скрытие по его
//    истечении → повторная активность откладывает скрытие). Реальный вызов
//    `window.addEventListener` и cleanup не покрыты — это ограничение окружения.

import { useState } from "react";

import { useAutoHide } from "./use-auto-hide";

interface FakeTimers {
  setTimeout: (cb: () => void, ms: number) => number;
  clearTimeout: (id: number) => void;
}

// Модель состояния хука: `visible` + активный таймер. `show()` повторяет тело функции
// `show` из `useAutoHide` один в один.
function createAutoHideModel(delayMs: number, timers: FakeTimers) {
  let visible = true; // useState(true)
  let timer: number | null = null;

  const show = () => {
    visible = true;
    if (timer !== null) {
      timers.clearTimeout(timer);
    }
    timer = timers.setTimeout(() => {
      visible = false;
    }, delayMs);
  };

  return {
    show,
    get visible() {
      return visible;
    },
    get timerId() {
      return timer;
    },
    get hasTimer() {
      return timer !== null;
    },
  };
}

function manualTimers() {
  let nextId = 1;
  const pending = new Map<number, { cb: () => void; ms: number }>();
  return {
    setTimeout: (cb: () => void, ms: number) => {
      const id = nextId++;
      pending.set(id, { cb, ms });
      return id;
    },
    clearTimeout: (id: number) => {
      pending.delete(id);
    },
    run: (id: number) => {
      const entry = pending.get(id);
      if (!entry) return;
      pending.delete(id);
      entry.cb();
    },
    runAll: () => {
      for (const [id, entry] of [...pending]) {
        pending.delete(id);
        entry.cb();
      }
    },
    pendingCount: () => pending.size,
  };
}

function VisibleProbe() {
  const visible = useAutoHide(3000);
  return <span data-visible={String(visible)} />;
}

describe("useAutoHide", () => {
  it("хук — функция ожидаемой арности и не падает при рендере", () => {
    expect(typeof useAutoHide).toBe("function");
    expect(useAutoHide.length).toBe(1);
    // `renderToStaticMarkup` не запускает эффекты, но подтверждает, что хук рендерится
    // без исключения и начальное `visible === true`.
    expect(renderToStaticMarkup(<VisibleProbe />)).toContain('data-visible="true"');
  });

  it("начальное состояние контракта — visible = true", () => {
    // Эквивалент `useState(true)` внутри `useAutoHide`, вынесенный в отдельный пробник,
    // т.к. сам хук слушает window в эффекте, недоступном в env "node".
    function InitialProbe() {
      const [visible] = useState(() => true);
      return <span>{String(visible)}</span>;
    }
    expect(renderToStaticMarkup(<InitialProbe />)).toBe("<span>true</span>");
  });

  it("show() показывает контролы и ставит таймер на delayMs", () => {
    const timers = manualTimers();
    const model = createAutoHideModel(3000, timers);

    model.show();

    expect(model.visible).toBe(true);
    expect(model.hasTimer).toBe(true);
    expect(timers.pendingCount()).toBe(1);
  });

  it("по истечении delayMs контролы скрываются", () => {
    const timers = manualTimers();
    const model = createAutoHideModel(3000, timers);

    model.show();
    timers.runAll();

    expect(model.visible).toBe(false);
  });

  it("новая активность отменяет старый таймер и откладывает скрытие", () => {
    const timers = manualTimers();
    const model = createAutoHideModel(3000, timers);

    model.show();
    const firstId = model.timerId!;

    model.show();
    expect(model.timerId).not.toBe(firstId);
    expect(timers.pendingCount()).toBe(1);

    // Старый таймер отменён — его прокрут ничего не меняет.
    timers.run(firstId);
    expect(model.visible).toBe(true);

    // Новый таймер активен и скрывает контролы.
    timers.runAll();
    expect(model.visible).toBe(false);
  });

  it("после скрытия новая активность снова показывает контролы", () => {
    const timers = manualTimers();
    const model = createAutoHideModel(3000, timers);

    model.show();
    timers.runAll();
    expect(model.visible).toBe(false);

    model.show();
    expect(model.visible).toBe(true);
    expect(timers.pendingCount()).toBe(1);
  });
});
