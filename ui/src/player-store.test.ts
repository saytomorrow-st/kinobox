import { beforeEach, describe, expect, it, vi } from "vitest";

import type { Api, IpcError, Outcome, PlayerErrorDto, PlayerProgressDto, StreamQualityDto } from "./api";

// `listen` из Tauri заменяем моком: в node-окружении нет ни Tauri-рантайма, ни реального
// канала событий. Мок отдаёт колбэк наружу, чтобы тест эмулировал `player-progress` вручную,
// и возвращает unlisten-шпион для проверки отписки в `dispose()`.
const listenMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/event", () => ({ listen: listenMock }));

const { createPlayerStore } = await import("./player-store");

const ERROR_QUALITIES = "Не удалось получить список качеств.";
const ERROR_ACTION = "Не удалось выполнить действие плеера. Попробуйте ещё раз.";

function quality(label = "1080p", overrides: Partial<StreamQualityDto> = {}): StreamQualityDto {
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

// Настраивает мок `listen` так, чтобы можно было достать переданные колбэки и unlisten-шпион.
// `listen` вызывается для двух событий (`player-progress` и `player-error`), поэтому колбэки
// раскладываются по имени события.
function setupListen(): {
  unlisten: ReturnType<typeof vi.fn>;
  emitProgress: (payload: PlayerProgressDto) => void;
  emitError: (payload: PlayerErrorDto) => void;
} {
  const unlisten = vi.fn();
  const captured: Record<string, (event: { payload: unknown }) => void> = {};
  listenMock.mockImplementation(async (event: string, handler: (event: { payload: unknown }) => void) => {
    captured[event] = handler;
    return unlisten;
  });
  return {
    unlisten,
    emitProgress: (payload) => {
      const handler = captured["player-progress"];
      if (!handler) {
        throw new Error("player-progress ещё не подписан");
      }
      handler({ payload });
    },
    emitError: (payload) => {
      const handler = captured["player-error"];
      if (!handler) {
        throw new Error("player-error ещё не подписан");
      }
      handler({ payload });
    },
  };
}

beforeEach(() => {
  listenMock.mockReset();
  setupListen();
});

describe("init", () => {
  it("успешно грузит качества и выставляет первое как текущее", async () => {
    const qualitiesSpy = vi.fn<Api["playerQualities"]>(async () => ({ ok: true, value: [hd, sd] }));
    const store = createPlayerStore(fakeApi({ playerQualities: qualitiesSpy }));

    await store.getState().init();

    expect(qualitiesSpy).toHaveBeenCalledTimes(1);
    expect(store.getState()).toMatchObject({
      qualities: [hd, sd],
      currentQuality: hd,
      qualitiesLoading: false,
      error: null,
    });
  });

  it("пустой список качеств оставляет currentQuality = null", async () => {
    const store = createPlayerStore(
      fakeApi({ playerQualities: async () => ({ ok: true, value: [] }) }),
    );

    await store.getState().init();

    expect(store.getState()).toMatchObject({
      qualities: [],
      currentQuality: null,
      qualitiesLoading: false,
      error: null,
    });
  });

  it("ошибка загрузки качеств снимает загрузку и ставит ошибку", async () => {
    const store = createPlayerStore(
      fakeApi({ playerQualities: async () => failure("network", "нет сети") }),
    );

    await store.getState().init();

    expect(store.getState()).toMatchObject({
      qualitiesLoading: false,
      error: ERROR_QUALITIES,
      qualities: [],
    });
    expect(store.getState().error).not.toBe("");
  });

  it("подписывается на события player-progress и player-error по одному разу", async () => {
    const store = createPlayerStore(
      fakeApi({ playerQualities: async () => ({ ok: true, value: [hd] }) }),
    );

    await store.getState().init();

    const events = listenMock.mock.calls.map((call) => call[0]);
    expect(events).toEqual(["player-progress", "player-error"]);
  });

  it("эмулирует прогресс-событие и обновляет position/duration/paused", async () => {
    const { emitProgress } = setupListen();
    const store = createPlayerStore(
      fakeApi({ playerQualities: async () => ({ ok: true, value: [hd] }) }),
    );
    await store.getState().init();

    emitProgress({ position: 42, duration: 1800, paused: true });

    expect(store.getState()).toMatchObject({ position: 42, duration: 1800, paused: true });
  });

  it("эмулирует player-error и выставляет сообщение в error", async () => {
    const { emitError } = setupListen();
    const store = createPlayerStore(
      fakeApi({ playerQualities: async () => ({ ok: true, value: [hd] }) }),
    );
    await store.getState().init();

    emitError({ message: "Не удалось воспроизвести поток." });

    expect(store.getState().error).toBe("Не удалось воспроизвести поток.");
  });

  it("повторный вызов, пока первый ещё выполняется, не дублирует запросы", async () => {
    let releaseQualities!: (outcome: Outcome<StreamQualityDto[]>) => void;
    const qualitiesSpy = vi.fn(
      () => new Promise<Outcome<StreamQualityDto[]>>((resolve) => (releaseQualities = resolve)),
    );
    const store = createPlayerStore(fakeApi({ playerQualities: qualitiesSpy }));

    const first = store.getState().init();
    const second = store.getState().init();
    expect(store.getState().qualitiesLoading).toBe(true);

    releaseQualities({ ok: true, value: [hd] });
    await Promise.all([first, second]);

    expect(qualitiesSpy).toHaveBeenCalledTimes(1);
    expect(listenMock).toHaveBeenCalledTimes(2);
    expect(store.getState().qualities).toEqual([hd]);
  });

  it("повторный вызов после завершения не повторяет загрузку и подписку", async () => {
    const qualitiesSpy = vi.fn<Api["playerQualities"]>(async () => ({ ok: true, value: [hd] }));
    const store = createPlayerStore(fakeApi({ playerQualities: qualitiesSpy }));

    await store.getState().init();
    await store.getState().init();

    expect(qualitiesSpy).toHaveBeenCalledTimes(1);
    expect(listenMock).toHaveBeenCalledTimes(2);
  });
});

describe("dispose", () => {
  it("вызывает unlisten и позволяет init заново подписаться", async () => {
    const { unlisten } = setupListen();
    const store = createPlayerStore(
      fakeApi({ playerQualities: async () => ({ ok: true, value: [hd] }) }),
    );
    await store.getState().init();

    store.getState().dispose();
    expect(unlisten).toHaveBeenCalledTimes(2);

    await store.getState().init();
    expect(listenMock).toHaveBeenCalledTimes(4);
  });
});

describe("действия плеера", () => {
  it("play вызывает API и при успехе не ставит ошибку", async () => {
    const playSpy = vi.fn<Api["playerPlay"]>(async () => ({ ok: true, value: null }));
    const store = createPlayerStore(fakeApi({ playerPlay: playSpy }));

    await store.getState().play();

    expect(playSpy).toHaveBeenCalledTimes(1);
    expect(store.getState().error).toBeNull();
  });

  it("pause при ошибке ставит ошибку", async () => {
    const store = createPlayerStore(
      fakeApi({ playerPause: async () => failure("internal", "нет рендера") }),
    );

    await store.getState().pause();

    expect(store.getState().error).toBe(ERROR_ACTION);
  });

  it("seek передаёт секунды и при ошибке ставит ошибку", async () => {
    const seekSpy = vi.fn<Api["playerSeek"]>(async () => failure("internal", "нет рендера"));
    const store = createPlayerStore(fakeApi({ playerSeek: seekSpy }));

    await store.getState().seek(90);

    expect(seekSpy).toHaveBeenCalledWith(90);
    expect(store.getState().error).toBe(ERROR_ACTION);
  });

  it("setQuality при успехе обновляет currentQuality", async () => {
    const setQualitySpy = vi.fn<Api["playerSetQuality"]>(async () => ({ ok: true, value: null }));
    const store = createPlayerStore(fakeApi({ playerSetQuality: setQualitySpy }));

    await store.getState().setQuality(sd);

    expect(setQualitySpy).toHaveBeenCalledWith(sd);
    expect(store.getState()).toMatchObject({ currentQuality: sd, error: null });
  });

  it("setQuality при ошибке не меняет currentQuality и ставит ошибку", async () => {
    const store = createPlayerStore(
      fakeApi({ playerSetQuality: async () => failure("internal", "нет рендера") }),
    );

    await store.getState().setQuality(sd);

    expect(store.getState()).toMatchObject({ currentQuality: null, error: ERROR_ACTION });
  });

  it("setVolume оптимистично выставляет громкость даже при ошибке API", async () => {
    const store = createPlayerStore(
      fakeApi({ playerSetVolume: async () => failure("internal", "нет рендера") }),
    );

    await store.getState().setVolume(0.3);

    expect(store.getState()).toMatchObject({ volume: 0.3, error: ERROR_ACTION });
  });
});

describe("close", () => {
  it("отписывается от события и останавливает плеер", async () => {
    const { unlisten } = setupListen();
    const stopSpy = vi.fn<Api["playerStop"]>(async () => ({ ok: true, value: null }));
    const store = createPlayerStore(
      fakeApi({
        playerQualities: async () => ({ ok: true, value: [hd] }),
        playerStop: stopSpy,
      }),
    );
    await store.getState().init();

    await store.getState().close();

    expect(unlisten).toHaveBeenCalledTimes(2);
    expect(stopSpy).toHaveBeenCalledTimes(1);
    expect(store.getState().error).toBeNull();
  });
});
