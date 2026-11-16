import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { MirrorDto } from "../api";
import { MirrorsView, normalizeMirrorInput, type MirrorsViewProps } from "./MirrorsView";

function mirror(url: string, overrides: Partial<MirrorDto> = {}): MirrorDto {
  return {
    url,
    state: "unknown",
    reason: null,
    latencyMs: null,
    active: false,
    standby: false,
    ...overrides,
  };
}

const mirrors: MirrorDto[] = [
  mirror("https://kvk.pub/", { state: "needsLogin", latencyMs: 142, active: true }),
  mirror("https://rezka.fi/", { state: "open", latencyMs: 98 }),
  mirror("https://rezka.ag/", { state: "challenge" }),
  mirror("https://old.example/", { state: "dead", reason: "таймаут" }),
  mirror("https://standby-rezka.tv/", { standby: true }),
];

function render(props: Partial<MirrorsViewProps> = {}): string {
  return renderToStaticMarkup(
    <MirrorsView
      open
      onToggle={() => {}}
      mirrors={mirrors}
      checked={false}
      busy="idle"
      error={null}
      onCheck={() => {}}
      onSelect={() => {}}
      onAdd={async () => true}
      {...props}
    />,
  );
}

describe("MirrorsView", () => {
  it("показывает хосты без протокола и подписи состояний", () => {
    const html = render();
    for (const host of ["kvk.pub", "rezka.fi", "rezka.ag", "old.example", "standby-rezka.tv"]) {
      expect(html).toContain(host);
    }
    expect(html).not.toContain("https://kvk.pub");
    for (const label of ["Доступно", "Нужен вход", "Проверка на бота", "Недоступно", "Не проверено"]) {
      expect(html).toContain(label);
    }
    expect(html).toContain("резервный");
  });

  it("красит индикаторы по состоянию", () => {
    const html = render();
    for (const dot of ["bg-status-open", "bg-status-login", "bg-status-challenge", "bg-status-dead", "bg-status-unknown"]) {
      expect(html).toContain(dot);
    }
  });

  it("показывает задержку и отмечает активное зеркало", () => {
    const html = render();
    expect(html).toContain("142 мс");
    expect(html).toContain("98 мс");
    expect(html.match(/aria-current="true"/g)).toHaveLength(1);
    expect(html).toContain("Активное зеркало");
  });

  it("в свёрнутом виде показывает только заголовок и активный хост", () => {
    const html = render({ open: false });
    expect(html).toContain('aria-expanded="false"');
    expect(html).toContain("kvk.pub");
    expect(html).not.toContain("rezka.fi");
    expect(html).not.toContain("Проверить");
  });

  it("во время проверки блокирует кнопку и показывает загрузку", () => {
    const html = render({ busy: "checking" });
    expect(html).toContain("Проверяем…");
    expect(html).toContain("animate-spin");
    expect(html).toMatch(/<button[^>]*disabled=""[^>]*>[^]*?Проверяем…/);
  });

  it("после первой проверки предлагает проверить снова", () => {
    expect(render({ checked: true })).toContain("Проверить снова");
    expect(render({ checked: false })).toContain(">Проверить<");
  });

  it("выводит ошибку добавления в role=alert и не трогает список", () => {
    const html = render({ error: "Некорректный адрес зеркала" });
    expect(html).toMatch(/role="alert"[^>]*>Некорректный адрес зеркала</);
    expect(html).toContain("rezka.fi");
  });

  it("поле адреса не заполняется автоматически", () => {
    const html = render();
    expect(html).toContain('autoComplete="off"');
    expect(html).toContain('aria-label="Адрес зеркала"');
  });
});

describe("normalizeMirrorInput", () => {
  it("дописывает https:// к голому домену", () => {
    expect(normalizeMirrorInput("  kvk.pub ")).toBe("https://kvk.pub");
  });

  it("не трогает адрес со схемой", () => {
    expect(normalizeMirrorInput("http://kvk.pub")).toBe("http://kvk.pub");
  });

  it("пустой ввод остаётся пустым, чтобы бэкенд вернул invalidInput", () => {
    expect(normalizeMirrorInput("   ")).toBe("");
  });
});
