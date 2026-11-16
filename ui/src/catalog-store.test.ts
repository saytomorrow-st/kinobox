import { describe, expect, it, vi } from "vitest";

import type { Api, IpcError, MediaItemDto, Outcome } from "./api";
import { createCatalogStore } from "./catalog-store";

const ERROR_MESSAGE = "Не удалось загрузить каталог. Попробуйте ещё раз.";

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

const first = [item(1), item(2), item(3)];
const next = [item(4), item(5)];

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

describe("load", () => {
  it("успех сбрасывает страницу, очищает поиск и кладёт элементы", async () => {
    const catalog = vi.fn(async () => ({ ok: true as const, value: first }));
    const store = createCatalogStore(fakeApi({ catalog }));
    await store.getState().search("драма");
    await store.getState().load();

    expect(store.getState()).toMatchObject({
      items: first,
      page: 1,
      hasMore: true,
      loading: false,
      query: "",
      error: null,
    });
  });

  it("пустой ответ выключает догрузку", async () => {
    const store = createCatalogStore(fakeApi({ catalog: async () => ({ ok: true, value: [] }) }));
    await store.getState().load();
    expect(store.getState()).toMatchObject({ items: [], hasMore: false, loading: false });
  });

  it("передаёт жанр в api.catalog и сохраняет его", async () => {
    const catalog = vi.fn<Api["catalog"]>(async () => ({ ok: true, value: first }));
    const store = createCatalogStore(fakeApi({ catalog }));
    await store.getState().load("series");

    expect(catalog).toHaveBeenCalledWith("series", 1);
    expect(store.getState().kind).toBe("series");
  });

  it("не передавая жанр, сохраняет прежний", async () => {
    const catalog = vi.fn(fakeApi().catalog);
    const store = createCatalogStore(fakeApi({ catalog }));
    await store.getState().load("anime");
    await store.getState().load();

    expect(catalog).toHaveBeenLastCalledWith("anime", 1);
    expect(store.getState().kind).toBe("anime");
  });

  it("ошибка ставит общий текст и не трогает элементы", async () => {
    const catalog = vi
      .fn<Api["catalog"]>()
      .mockResolvedValueOnce({ ok: true, value: first })
      .mockResolvedValueOnce(failure("network", "нет сети"));
    const store = createCatalogStore(fakeApi({ catalog }));
    await store.getState().load();
    await store.getState().load();

    expect(store.getState()).toMatchObject({ loading: false, error: ERROR_MESSAGE, items: first });
  });

  it("не запускает вторую загрузку, пока идёт первая", async () => {
    let release!: () => void;
    const pending = new Promise<void>((resolve) => {
      release = resolve;
    });
    const catalog = vi.fn(async () => {
      await pending;
      return { ok: true as const, value: first };
    });
    const store = createCatalogStore(fakeApi({ catalog }));

    const call = store.getState().load();
    expect(store.getState().loading).toBe(true);
    await store.getState().load();
    release();
    await call;

    expect(catalog).toHaveBeenCalledTimes(1);
    expect(store.getState()).toMatchObject({ items: first, loading: false });
  });
});

describe("loadMore", () => {
  async function loaded(store: ReturnType<typeof createCatalogStore>) {
    await store.getState().load();
  }

  it("добавляет элементы в конец и увеличивает страницу", async () => {
    const catalog = vi
      .fn<Api["catalog"]>()
      .mockResolvedValueOnce({ ok: true, value: first })
      .mockResolvedValueOnce({ ok: true, value: next });
    const store = createCatalogStore(fakeApi({ catalog }));
    await loaded(store);
    await store.getState().loadMore();

    expect(catalog).toHaveBeenLastCalledWith(null, 2);
    expect(store.getState()).toMatchObject({
      items: [...first, ...next],
      page: 2,
      hasMore: true,
      loadingMore: false,
    });
  });

  it("пустая страница выключает hasMore", async () => {
    const catalog = vi
      .fn<Api["catalog"]>()
      .mockResolvedValueOnce({ ok: true, value: first })
      .mockResolvedValueOnce({ ok: true, value: [] });
    const store = createCatalogStore(fakeApi({ catalog }));
    await loaded(store);
    await store.getState().loadMore();

    expect(store.getState()).toMatchObject({ items: first, page: 2, hasMore: false });
  });

  it("ничего не делает во время основной загрузки", async () => {
    let release!: () => void;
    const pending = new Promise<void>((resolve) => {
      release = resolve;
    });
    const catalog = vi.fn(async () => {
      await pending;
      return { ok: true as const, value: first };
    });
    const store = createCatalogStore(fakeApi({ catalog }));
    const call = store.getState().load();
    await store.getState().loadMore();
    release();
    await call;

    expect(catalog).toHaveBeenCalledTimes(1);
    expect(store.getState().page).toBe(1);
  });

  it("ничего не делает без hasMore", async () => {
    const catalog = vi.fn(async () => ({ ok: true as const, value: [] }));
    const store = createCatalogStore(fakeApi({ catalog }));
    await loaded(store);
    await store.getState().loadMore();

    expect(catalog).toHaveBeenCalledTimes(1);
  });

  it("ничего не делает при активном поиске", async () => {
    const catalog = vi.fn(async () => ({ ok: true as const, value: first }));
    const store = createCatalogStore(fakeApi({ catalog }));
    await loaded(store);
    await store.getState().search("драма");
    await store.getState().loadMore();

    expect(catalog).toHaveBeenCalledTimes(1);
  });

  it("повторный вызов возвращает тот же запрос и не дублирует api.catalog", async () => {
    let release!: () => void;
    const pending = new Promise<void>((resolve) => {
      release = resolve;
    });
    const catalog = vi
      .fn<Api["catalog"]>()
      .mockResolvedValueOnce({ ok: true, value: first })
      .mockImplementationOnce(async () => {
        await pending;
        return { ok: true, value: next };
      });
    const store = createCatalogStore(fakeApi({ catalog }));
    await loaded(store);

    const call = store.getState().loadMore();
    expect(store.getState().loadingMore).toBe(true);
    await store.getState().loadMore();
    release();
    await call;

    expect(catalog).toHaveBeenCalledTimes(2);
    expect(store.getState()).toMatchObject({ items: [...first, ...next], page: 2, loadingMore: false });
  });

  it("ошибка сохраняет элементы и страницу", async () => {
    const catalog = vi
      .fn<Api["catalog"]>()
      .mockResolvedValueOnce({ ok: true, value: first })
      .mockResolvedValueOnce(failure("network", "нет сети"));
    const store = createCatalogStore(fakeApi({ catalog }));
    await loaded(store);
    await store.getState().loadMore();

    expect(store.getState()).toMatchObject({
      items: first,
      page: 1,
      loadingMore: false,
      error: ERROR_MESSAGE,
    });
  });
});

describe("search", () => {
  it("заменяет элементы результатами поиска", async () => {
    const search = vi.fn<Api["search"]>(async () => ({ ok: true, value: next }));
    const store = createCatalogStore(fakeApi({ search }));
    await store.getState().load();
    await store.getState().search("  драма  ");

    expect(search).toHaveBeenCalledWith("драма");
    expect(store.getState()).toMatchObject({
      items: next,
      page: 1,
      hasMore: false,
      loading: false,
      query: "драма",
      error: null,
    });
  });

  it("пустой запрос грузит каталог вместо поиска", async () => {
    const catalog = vi.fn(fakeApi().catalog);
    const search = vi.fn(fakeApi().search);
    const store = createCatalogStore(fakeApi({ catalog, search }));
    await store.getState().load("cartoon");
    await store.getState().search("   ");

    expect(search).not.toHaveBeenCalled();
    expect(catalog).toHaveBeenLastCalledWith("cartoon", 1);
    expect(store.getState()).toMatchObject({ query: "", loading: false });
  });

  it("ошибка ставит общий текст и снимает загрузку", async () => {
    const store = createCatalogStore(fakeApi({ search: async () => failure("network", "нет сети") }));
    await store.getState().search("драма");

    expect(store.getState()).toMatchObject({ loading: false, error: ERROR_MESSAGE });
  });

  it("не запускает поиск, пока идёт загрузка", async () => {
    let release!: () => void;
    const pending = new Promise<void>((resolve) => {
      release = resolve;
    });
    const catalog = vi.fn(async () => {
      await pending;
      return { ok: true as const, value: first };
    });
    const search = vi.fn(fakeApi().search);
    const store = createCatalogStore(fakeApi({ catalog, search }));

    const call = store.getState().load();
    await store.getState().search("драма");
    release();
    await call;

    expect(search).not.toHaveBeenCalled();
  });
});
