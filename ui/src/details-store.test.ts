import { describe, expect, it, vi } from "vitest";

import type {
  Api,
  EpisodeDto,
  IpcError,
  MediaDetailsDto,
  MediaItemDto,
  Outcome,
  StreamQualityDto,
} from "./api";
import { createDetailsStore } from "./details-store";

const ERROR_DETAILS = "Не удалось загрузить данные. Попробуйте ещё раз.";
const ERROR_EPISODES = "Не удалось загрузить список серий. Попробуйте ещё раз.";
const ERROR_LAUNCH = "Не удалось запустить воспроизведение. Попробуйте ещё раз.";

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
    description: "Описание",
    genres: ["драма"],
    countries: ["США"],
    durationMinutes: 120,
    translators: [{ id: 1, name: "Дубляж" }],
    seasons: [],
    ...overrides,
  };
}

function episode(seasonId: number, episodeId: number, title = `Серия ${episodeId}`): EpisodeDto {
  return { seasonId, episodeId, title };
}

function quality(label = "1080p"): StreamQualityDto {
  return {
    label,
    url: "https://cdn.example/stream.m3u8",
    premium: false,
    hls: null,
    fallbacks: [],
  };
}

function fakeApi(overrides: Partial<Api> = {}): Api {
  return {
    boot: async () => ({ ok: true, value: { state: "needsLogin", email: null } }),
    login: async () => ({ ok: true, value: { userId: 1, username: "kino" } }),
    logout: async () => ({ ok: true, value: null }),
    session: async () => null,
    mirrors: async () => ({ ok: true, value: [] }),
    checkMirrors: async () => ({ ok: true, value: [] }),
    addMirror: async () => ({ ok: false, error: { kind: "internal", message: "не вызывается" } }),
    selectMirror: async () => ({ ok: false, error: { kind: "internal", message: "не вызывается" } }),
    catalog: async () => ({ ok: true, value: [] }),
    search: async () => ({ ok: true, value: [] }),
    details: async () => ({ ok: false, error: { kind: "internal", message: "не вызывается" } }),
    episodes: async () => ({ ok: false, error: { kind: "internal", message: "не вызывается" } }),
    streams: async () => ({ ok: false, error: { kind: "internal", message: "не вызывается" } }),
    playerLoad: async () => ({ ok: false, error: { kind: "internal", message: "не вызывается" } }),
    playerQualities: async () => ({ ok: false, error: { kind: "internal", message: "не вызывается" } }),
    playerPlay: async () => ({ ok: false, error: { kind: "internal", message: "не вызывается" } }),
    playerPause: async () => ({ ok: false, error: { kind: "internal", message: "не вызывается" } }),
    playerSeek: async () => ({ ok: false, error: { kind: "internal", message: "не вызывается" } }),
    playerSetQuality: async () => ({ ok: false, error: { kind: "internal", message: "не вызывается" } }),
    playerSetVolume: async () => ({ ok: false, error: { kind: "internal", message: "не вызывается" } }),
    playerStop: async () => ({ ok: false, error: { kind: "internal", message: "не вызывается" } }),
    quit: async () => {},
    ...overrides,
  };
}

function failure(kind: IpcError["kind"], message: string): Outcome<never> {
  return { ok: false, error: { kind, message } };
}

const film = item(1, { kind: "movie", title: "Фильм" });
const series = item(2, { kind: "series", title: "Сериал" });

const filmDetails = details(film, { seasons: [] });
const seriesDetails = details(series, {
  seasons: [
    { number: 1, episodes: [1, 2] },
    { number: 2, episodes: [1, 2, 3] },
  ],
});
const seriesEpisodes = [episode(1, 1), episode(1, 2), episode(2, 1), episode(2, 2), episode(2, 3)];

describe("openFor", () => {
  it("открывает карточку, сбрасывает состояние и грузит детали фильма", async () => {
    const detailsSpy = vi.fn<Api["details"]>(async () => ({ ok: true, value: filmDetails }));
    const episodesSpy = vi.fn<Api["episodes"]>(async () => ({ ok: true, value: [] }));
    const store = createDetailsStore(fakeApi({ details: detailsSpy, episodes: episodesSpy }));

    const pending = store.getState().openFor(film);
    expect(store.getState()).toMatchObject({
      open: true,
      loading: true,
      item: film,
      details: null,
      error: null,
      translatorId: null,
      episodes: [],
      episodesLoading: false,
      episodeIndex: 0,
      launching: false,
      launchError: null,
    });
    await pending;

    expect(detailsSpy).toHaveBeenCalledWith(film);
    expect(store.getState()).toMatchObject({
      open: true,
      loading: false,
      details: filmDetails,
      translatorId: 1,
      error: null,
    });
  });

  it("берёт id первого перевода, а без переводов оставляет null", async () => {
    const withTranslators = details(film, {
      translators: [
        { id: 7, name: "Первый" },
        { id: 9, name: "Второй" },
      ],
    });
    const store = createDetailsStore(
      fakeApi({ details: async () => ({ ok: true, value: withTranslators }) }),
    );
    await store.getState().openFor(film);
    expect(store.getState().translatorId).toBe(7);

    const noTranslators = details(film, { translators: [] });
    const emptyStore = createDetailsStore(
      fakeApi({ details: async () => ({ ok: true, value: noTranslators }) }),
    );
    await emptyStore.getState().openFor(film);
    expect(emptyStore.getState().translatorId).toBeNull();
  });

  it("сбрасывает прошлое состояние при повторном открытии", async () => {
    const resolvers: Array<(outcome: Outcome<MediaDetailsDto>) => void> = [];
    const detailsSpy = vi.fn(
      () =>
        new Promise<Outcome<MediaDetailsDto>>((resolve) => {
          resolvers.push(resolve);
        }),
    );
    const store = createDetailsStore(fakeApi({ details: detailsSpy }));

    const reopened = store.getState().openFor(film);
    resolvers.shift()?.({ ok: true, value: filmDetails });
    await reopened;
    store.getState().selectTranslator(99);
    expect(store.getState().translatorId).toBe(99);

    const again = store.getState().openFor(item(5));
    expect(store.getState()).toMatchObject({
      open: true,
      loading: true,
      item: item(5),
      details: null,
      translatorId: null,
      episodes: [],
      episodeIndex: 0,
      launching: false,
      launchError: null,
    });

    resolvers.shift()?.({ ok: true, value: details(item(5)) });
    await again;
  });

  it("ошибка деталей снимает загрузку и оставляет details пустыми", async () => {
    const store = createDetailsStore(
      fakeApi({ details: async () => failure("network", "нет сети") }),
    );
    await store.getState().openFor(film);

    expect(store.getState()).toMatchObject({
      loading: false,
      error: ERROR_DETAILS,
      details: null,
    });
    expect(store.getState().error).not.toBe("");
  });

  it("для фильма не запрашивает серии", async () => {
    const episodesSpy = vi.fn<Api["episodes"]>(async () => ({ ok: true, value: [] }));
    const store = createDetailsStore(
      fakeApi({ details: async () => ({ ok: true, value: filmDetails }), episodes: episodesSpy }),
    );
    await store.getState().openFor(film);

    expect(episodesSpy).not.toHaveBeenCalled();
    expect(store.getState().episodes).toEqual([]);
    expect(store.getState().episodesLoading).toBe(false);
  });

  it("для сериала грузит серии через episodesLoading и ставит index 0", async () => {
    let release!: (outcome: Outcome<EpisodeDto[]>) => void;
    const episodesSpy = vi.fn(
      () => new Promise<Outcome<EpisodeDto[]>>((resolve) => (release = resolve)),
    );
    const store = createDetailsStore(
      fakeApi({ details: async () => ({ ok: true, value: seriesDetails }), episodes: episodesSpy }),
    );

    const pending = store.getState().openFor(series);
    await vi.waitFor(() => {
      expect(store.getState().episodesLoading).toBe(true);
    });
    expect(store.getState().details).toEqual(seriesDetails);
    expect(store.getState().loading).toBe(false);

    release({ ok: true, value: seriesEpisodes });
    await pending;

    expect(episodesSpy).toHaveBeenCalledWith(seriesDetails);
    expect(store.getState()).toMatchObject({
      episodes: seriesEpisodes,
      episodesLoading: false,
      episodeIndex: 0,
      error: null,
    });
  });

  it("ошибка серий снимает episodesLoading и ставит ошибку", async () => {
    const store = createDetailsStore(
      fakeApi({
        details: async () => ({ ok: true, value: seriesDetails }),
        episodes: async () => failure("network", "нет сети"),
      }),
    );
    await store.getState().openFor(series);

    expect(store.getState()).toMatchObject({
      episodesLoading: false,
      error: ERROR_EPISODES,
      episodes: [],
    });
    expect(store.getState().error).not.toBe("");
  });

  it("отбрасывает устаревший ответ прошлого вызова openFor", async () => {
    const itemA = item(10);
    const itemB = item(20);
    const detailsA = details(itemA, { description: "A" });
    const detailsB = details(itemB, { description: "B" });

    const resolvers: Array<(outcome: Outcome<MediaDetailsDto>) => void> = [];
    const detailsSpy = vi.fn(
      () =>
        new Promise<Outcome<MediaDetailsDto>>((resolve) => {
          resolvers.push(resolve);
        }),
    );
    const store = createDetailsStore(fakeApi({ details: detailsSpy }));

    const firstCall = store.getState().openFor(itemA);
    const secondCall = store.getState().openFor(itemB);
    const [resolveA, resolveB] = resolvers as [
      (outcome: Outcome<MediaDetailsDto>) => void,
      (outcome: Outcome<MediaDetailsDto>) => void,
    ];

    // Сначала завершается второй запрос, затем первый — ответ первого должен быть проигнорирован.
    resolveB({ ok: true, value: detailsB });
    await secondCall;
    resolveA({ ok: true, value: detailsA });
    await firstCall;

    expect(store.getState()).toMatchObject({
      item: itemB,
      details: detailsB,
      loading: false,
      error: null,
    });
  });
});

describe("close", () => {
  it("закрывает карточку, не сбрасывая данные", async () => {
    const store = createDetailsStore(
      fakeApi({ details: async () => ({ ok: true, value: filmDetails }) }),
    );
    await store.getState().openFor(film);

    store.getState().close();

    expect(store.getState()).toMatchObject({
      open: false,
      details: filmDetails,
      item: film,
      loading: false,
    });
  });
});

describe("selectTranslator", () => {
  it("выставляет выбранный перевод", () => {
    const store = createDetailsStore(fakeApi());
    store.getState().selectTranslator(42);
    expect(store.getState().translatorId).toBe(42);
  });
});

describe("selectEpisode", () => {
  async function withSeries() {
    const store = createDetailsStore(
      fakeApi({
        details: async () => ({ ok: true, value: seriesDetails }),
        episodes: async () => ({ ok: true, value: seriesEpisodes }),
      }),
    );
    await store.getState().openFor(series);
    return store;
  }

  it("выбирает серию в пределах диапазона", async () => {
    const store = await withSeries();
    store.getState().selectEpisode(3);
    expect(store.getState().episodeIndex).toBe(3);
  });

  it("игнорирует отрицательный индекс и индекс за пределами списка", async () => {
    const store = await withSeries();
    store.getState().selectEpisode(2);
    store.getState().selectEpisode(-1);
    expect(store.getState().episodeIndex).toBe(2);
    store.getState().selectEpisode(seriesEpisodes.length);
    expect(store.getState().episodeIndex).toBe(2);
  });
});

describe("play", () => {
  it("для фильма берёт первое качество и передаёт эпизод null", async () => {
    const streamsSpy = vi.fn<Api["streams"]>(async () => ({ ok: true, value: [quality()] }));
    const playerLoadSpy = vi.fn<Api["playerLoad"]>(async () => ({ ok: true, value: null }));
    const store = createDetailsStore(
      fakeApi({
        details: async () => ({ ok: true, value: filmDetails }),
        streams: streamsSpy,
        playerLoad: playerLoadSpy,
      }),
    );
    await store.getState().openFor(film);

    await store.getState().play();

    expect(streamsSpy).toHaveBeenCalledWith(film, 1, null);
    expect(playerLoadSpy).toHaveBeenCalledWith(quality(), [quality()]);
    expect(store.getState()).toMatchObject({ launching: false, open: false, launchError: null });
  });

  it("для сериала передаёт выбранную серию", async () => {
    const streamsSpy = vi.fn<Api["streams"]>(async () => ({ ok: true, value: [quality()] }));
    const playerLoadSpy = vi.fn<Api["playerLoad"]>(async () => ({ ok: true, value: null }));
    const store = createDetailsStore(
      fakeApi({
        details: async () => ({ ok: true, value: seriesDetails }),
        episodes: async () => ({ ok: true, value: seriesEpisodes }),
        streams: streamsSpy,
        playerLoad: playerLoadSpy,
      }),
    );
    await store.getState().openFor(series);
    store.getState().selectEpisode(3);

    await store.getState().play();

    expect(streamsSpy).toHaveBeenCalledWith(series, 1, seriesEpisodes[3]);
    expect(playerLoadSpy).toHaveBeenCalledWith(quality(), [quality()]);
    expect(store.getState()).toMatchObject({ launching: false, open: false, launchError: null });
  });

  it("пустой список качеств даёт ошибку запуска и не закрывает карточку", async () => {
    const playerLoadSpy = vi.fn<Api["playerLoad"]>(async () => ({ ok: true, value: null }));
    const store = createDetailsStore(
      fakeApi({
        details: async () => ({ ok: true, value: filmDetails }),
        streams: async () => ({ ok: true, value: [] }),
        playerLoad: playerLoadSpy,
      }),
    );
    await store.getState().openFor(film);

    await store.getState().play();

    expect(playerLoadSpy).not.toHaveBeenCalled();
    expect(store.getState()).toMatchObject({ launching: false, launchError: ERROR_LAUNCH, open: true });
    expect(store.getState().launchError).not.toBe("");
  });

  it("ошибка streams даёт ошибку запуска и не закрывает карточку", async () => {
    const store = createDetailsStore(
      fakeApi({
        details: async () => ({ ok: true, value: filmDetails }),
        streams: async () => failure("network", "нет сети"),
      }),
    );
    await store.getState().openFor(film);

    await store.getState().play();

    expect(store.getState()).toMatchObject({ launching: false, launchError: ERROR_LAUNCH, open: true });
  });

  it("ошибка playerLoad даёт ошибку запуска и не закрывает карточку", async () => {
    const store = createDetailsStore(
      fakeApi({
        details: async () => ({ ok: true, value: filmDetails }),
        streams: async () => ({ ok: true, value: [quality()] }),
        playerLoad: async () => failure("internal", "плеер не запустился"),
      }),
    );
    await store.getState().openFor(film);

    await store.getState().play();

    expect(store.getState()).toMatchObject({ launching: false, launchError: ERROR_LAUNCH, open: true });
  });

  it("для сериала без выбранной серии ничего не вызывает", async () => {
    const streamsSpy = vi.fn<Api["streams"]>(async () => ({ ok: true, value: [quality()] }));
    const store = createDetailsStore(
      fakeApi({
        details: async () => ({ ok: true, value: seriesDetails }),
        episodes: async () => ({ ok: true, value: [] }),
        streams: streamsSpy,
      }),
    );
    await store.getState().openFor(series);

    await store.getState().play();

    expect(streamsSpy).not.toHaveBeenCalled();
    expect(store.getState()).toMatchObject({ launching: false, open: true, launchError: null });
  });

  it("не запускает повторный запуск, пока идёт первый", async () => {
    let release!: (outcome: Outcome<StreamQualityDto[]>) => void;
    const streamsSpy = vi.fn(
      () => new Promise<Outcome<StreamQualityDto[]>>((resolve) => (release = resolve)),
    );
    const playerLoadSpy = vi.fn<Api["playerLoad"]>(async () => ({ ok: true, value: null }));
    const store = createDetailsStore(
      fakeApi({
        details: async () => ({ ok: true, value: filmDetails }),
        streams: streamsSpy,
        playerLoad: playerLoadSpy,
      }),
    );
    await store.getState().openFor(film);

    const first = store.getState().play();
    expect(store.getState().launching).toBe(true);
    await store.getState().play();

    release({ ok: true, value: [quality()] });
    await first;

    expect(streamsSpy).toHaveBeenCalledTimes(1);
    expect(playerLoadSpy).toHaveBeenCalledTimes(1);
    expect(store.getState()).toMatchObject({ launching: false, open: false });
  });
});
