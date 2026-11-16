import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

import type { StreamQualityDto } from "../api";
import { PlayerControlsView, type PlayerControlsViewProps } from "./PlayerControlsView";

// `PlayerControlsView` в эффектах дёргает `@tauri-apps/api/window` (`getCurrentWindow`).
// В node-окружении Tauri-рантайма нет, поэтому подменяем окно заглушкой. Компонент вызывает
// его только из обработчиков `onMouseEnter`/`onMouseLeave`, которые `renderToStaticMarkup`
// не выполняет, но мок всё равно нужен, чтобы импорт модуля не упал.
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    setIgnoreCursorEvents: vi.fn(async () => {}),
  }),
}));

function quality(label: string, overrides: Partial<StreamQualityDto> = {}): StreamQualityDto {
  return {
    label,
    url: `https://cdn.example/${label}.m3u8`,
    premium: false,
    hls: null,
    fallbacks: [],
    ...overrides,
  };
}

const hd = quality("1080p");
const sd = quality("720p");
const qualities = [hd, sd];

function render(props: Partial<PlayerControlsViewProps> = {}): string {
  return renderToStaticMarkup(
    <PlayerControlsView
      qualities={qualities}
      qualitiesLoading={false}
      currentQuality={hd}
      position={50}
      duration={100}
      paused={false}
      volume={1}
      error={null}
      onPlay={() => {}}
      onPause={() => {}}
      onSeek={() => {}}
      onSelectQuality={() => {}}
      onSetVolume={() => {}}
      onClose={() => {}}
      {...props}
    />,
  );
}

describe("PlayerControlsView", () => {
  it("показывает кнопку play, когда видео на паузе", () => {
    const html = render({ paused: true });
    expect(html).toContain('aria-label="Воспроизвести"');
    expect(html).not.toContain('aria-label="Пауза"');
  });

  it("показывает кнопку pause, когда видео играет", () => {
    const html = render({ paused: false });
    expect(html).toContain('aria-label="Пауза"');
    expect(html).not.toContain('aria-label="Воспроизвести"');
  });

  it("показывает спиннер, пока грузятся качества", () => {
    const html = render({ qualitiesLoading: true });
    expect(html).toContain("animate-spin");
    expect(html).not.toContain('aria-label="Качество"');
  });

  it("скрывает picker качества, если список пуст", () => {
    const html = render({ qualities: [], currentQuality: null });
    expect(html).not.toContain('aria-label="Качество"');
    expect(html).not.toContain("animate-spin");
  });

  it("показывает picker качества с текущим выбранным значением", () => {
    const html = render();
    expect(html).toContain('aria-label="Качество"');
    expect(html).toContain("1080p");
    expect(html).toContain("720p");
    expect(html).toMatch(/<select[^>]*aria-label="Качество"[^]*<\/select>/);
    expect(html).toContain('<option value="1080p"');
  });

  it("выводит ошибку в role=alert и не выводит, когда ошибки нет", () => {
    const withError = render({ error: "Не удалось перемотать" });
    expect(withError).toMatch(/role="alert"[^>]*>Не удалось перемотать</);

    const withoutError = render({ error: null });
    expect(withoutError).not.toContain('role="alert"');
    expect(withoutError).not.toContain("Не удалось перемотать");
  });

  it("заполняет прогресс-бар пропорционально position/duration", () => {
    const html = render({ position: 50, duration: 100 });
    expect(html).toContain("width:50%");
    expect(html).toContain('aria-valuenow="50"');
    expect(html).toContain('aria-valuemax="100"');
  });

  it("прогресс равен нулю при неизвестном времени", () => {
    const html = render({ position: null, duration: null });
    expect(html).toContain("width:0%");
    expect(html).toContain('aria-valuenow="0"');
  });

  it("форматирует текущую позицию и длительность через formatTime", () => {
    const html = render({ position: 65, duration: 3600 });
    expect(html).toContain("01:05");
    expect(html).toContain("1:00:00");
  });

  it("отражает громкость в инпуте", () => {
    const html = render({ volume: 0.35 });
    expect(html).toContain('aria-label="Громкость"');
    expect(html).toContain('value="0.35"');
  });

  it("выводит кнопку закрытия плеера", () => {
    expect(render()).toContain('aria-label="Закрыть плеер"');
  });
});
