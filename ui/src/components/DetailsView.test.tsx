import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { EpisodeDto, MediaDetailsDto, MediaItemDto } from "../api";
import { DetailsView, type DetailsViewProps } from "./DetailsView";

function item(id: number, overrides: Partial<MediaItemDto> = {}): MediaItemDto {
  return {
    id,
    title: `Фильм ${id}`,
    originalTitle: null,
    year: 2020,
    kind: "movie",
    posterUrl: null,
    pageUrl: `https://rezka.ag/film/${id}`,
    rating: null,
    ...overrides,
  };
}

function details(base: MediaItemDto, overrides: Partial<MediaDetailsDto> = {}): MediaDetailsDto {
  return {
    item: base,
    description: "Описание фильма",
    genres: ["драма", "триллер"],
    countries: ["США", "Канада"],
    durationMinutes: 125,
    translators: [
      { id: 1, name: "Дубляж" },
      { id: 2, name: "Многоголосый" },
    ],
    seasons: [],
    ...overrides,
  };
}

function episode(seasonId: number, episodeId: number): EpisodeDto {
  return { seasonId, episodeId, title: `Серия ${episodeId}` };
}

const filmItem = item(1, { title: "Начало", year: 2010, rating: 8.8 });
const filmDetails = details(filmItem);

const seriesItem = item(2, { title: "Сериал", kind: "series" });
const seriesDetails = details(seriesItem, {
  seasons: [
    { number: 1, episodes: [1, 2] },
    { number: 2, episodes: [1, 2, 3] },
  ],
});
const seriesEpisodes = [episode(1, 1), episode(1, 2), episode(2, 1)];

function render(props: Partial<DetailsViewProps> = {}): string {
  return renderToStaticMarkup(
    <DetailsView
      item={filmItem}
      details={filmDetails}
      loading={false}
      error={null}
      translatorId={1}
      episodes={[]}
      episodesLoading={false}
      episodeIndex={0}
      launching={false}
      launchError={null}
      onClose={() => {}}
      onSelectTranslator={() => {}}
      onSelectEpisode={() => {}}
      onPlay={() => {}}
      {...props}
    />,
  );
}

function renderSeries(props: Partial<DetailsViewProps> = {}): string {
  return render({
    item: seriesItem,
    details: seriesDetails,
    episodes: seriesEpisodes,
    ...props,
  });
}

describe("DetailsView", () => {
  it("показывает загрузку вместо содержимого", () => {
    const html = render({ loading: true });
    expect(html).toContain("Загрузка…");
    expect(html).toContain("animate-spin");
    expect(html).not.toContain(filmDetails.description!);
    expect(html).not.toContain("<h2");
  });

  it("показывает текст ошибки без деталей", () => {
    const html = render({ error: "Нет соединения", details: null });
    expect(html).toMatch(/role="alert"[^>]*>Нет соединения</);
    expect(html).not.toContain("<h2");
  });

  it("показывает подпись по умолчанию, если нет ни деталей, ни ошибки", () => {
    const html = render({ details: null, item: null });
    expect(html).toContain("Не удалось загрузить данные. Попробуйте ещё раз.");
  });

  it("выводит название, описание, жанры, страны и переводы", () => {
    const html = render();
    expect(html).toContain("Начало");
    expect(html).toContain("Описание фильма");
    expect(html).toContain("драма, триллер");
    expect(html).toContain("США, Канада");
    expect(html).toContain("Дубляж");
    expect(html).toContain("Многоголосый");
    expect(html).toContain("★ 8.8");
  });

  it("форматирует длительность", () => {
    expect(render({ details: details(filmItem, { durationMinutes: 45 }) })).toContain("45 мин");
    expect(render({ details: details(filmItem, { durationMinutes: 120 }) })).toContain("2 ч");
    expect(render({ details: details(filmItem, { durationMinutes: 125 }) })).toContain("2 ч 5 мин");
  });

  it("подсвечивает активный перевод и снимает подсветку при null", () => {
    expect(
      render({ details: filmDetails, translatorId: 1 }),
    ).toContain(
      'aria-pressed="true" class="rounded-button px-3 py-1.5 text-card outline-none focus-visible:ring-2 focus-visible:ring-accent/70 bg-accent text-white"',
    );
    expect(render({ details: filmDetails, translatorId: null })).not.toContain(
      "bg-accent text-white",
    );
  });

  it("для фильма не выводит вкладки сезонов и серии", () => {
    const html = render();
    expect(html).not.toContain("Сезон");
    expect(html).not.toContain("Серия");
    expect(html).not.toContain('role="tablist"');
  });

  it("для сериала выводит вкладки сезонов и серии", () => {
    const html = renderSeries();
    expect(html).toContain("Сезон 1");
    expect(html).toContain("Сезон 2");
    expect(html).toContain('role="tablist"');
    expect(html.match(/role="tab"/g)).toHaveLength(2);
  });

  it("показывает загрузку серий во время загрузки", () => {
    const html = renderSeries({ episodesLoading: true, episodes: [] });
    expect(html).toContain("Загрузка серий…");
    expect(html).not.toContain("<ul");
  });

  it("показывает «Запуск…» и блокирует кнопку во время запуска", () => {
    const html = render({ launching: true });
    expect(html).toContain("Запуск…");
    expect(html).not.toContain("▶ Смотреть");
    expect(html).toMatch(/<button[^>]*disabled=""[^>]*>[^]*?Запуск…/);
  });

  it("показывает ошибку запуска в role=alert", () => {
    const html = render({ launchError: "Плеер не запустился" });
    expect(html).toContain("Плеер не запустился");
  });

  it("блокирует кнопку сериала, пока серии не загружены", () => {
    expect(renderSeries({ episodes: [] })).toMatch(/<button[^>]*disabled=""[^>]*>[^]*?Смотреть/);
    expect(render()).not.toMatch(/<button[^>]*disabled=""[^>]*>[^]*?Смотреть/);
  });
});
