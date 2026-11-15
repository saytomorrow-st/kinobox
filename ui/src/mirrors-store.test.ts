import { describe, expect, it, vi } from "vitest";

import type { Api, IpcError, MirrorDto, Outcome } from "./api";
import { createMirrorsStore, mirrorHost } from "./mirrors-store";

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

const initial = [mirror("https://kvk.pub/", { active: true }), mirror("https://rezka.fi/")];

function fakeApi(overrides: Partial<Api> = {}): Api {
  return {
    boot: async () => ({ ok: true, value: { state: "needsLogin", email: null } }),
    login: async () => ({ ok: true, value: { userId: 1, username: "kino" } }),
    logout: async () => ({ ok: true, value: null }),
    session: async () => null,
    mirrors: async () => ({ ok: true, value: initial }),
    checkMirrors: async () => ({ ok: true, value: initial }),
    addMirror: async (url) => ({ ok: true, value: mirror(url, { state: "open", active: true }) }),
    selectMirror: async (url) => ({ ok: true, value: mirror(url, { state: "open", active: true }) }),
    catalog: async () => ({ ok: true, value: [] }),
    search: async () => ({ ok: true, value: [] }),
    quit: async () => {},
    ...overrides,
  };
}

function failure(kind: IpcError["kind"], message: string): Outcome<never> {
  return { ok: false, error: { kind, message } };
}

describe("load", () => {
  it("берёт список без проверки сети", async () => {
    const checkMirrors = vi.fn(fakeApi().checkMirrors);
    const store = createMirrorsStore(fakeApi({ checkMirrors }));
    await store.getState().load();
    expect(store.getState().mirrors).toEqual(initial);
    expect(store.getState().checked).toBe(false);
    expect(checkMirrors).not.toHaveBeenCalled();
  });

  it("повторная загрузка сохраняет результаты прошлой проверки", async () => {
    const checked = [
      mirror("https://kvk.pub/", { state: "needsLogin", latencyMs: 120, active: true }),
      mirror("https://rezka.fi/", { state: "dead", reason: "таймаут" }),
    ];
    const store = createMirrorsStore(fakeApi({ checkMirrors: async () => ({ ok: true, value: checked }) }));
    await store.getState().check();
    await store.getState().load();
    expect(store.getState().mirrors).toEqual(checked);
  });
});

describe("check", () => {
  it("вызывает checkMirrors, показывает загрузку и обновляет список", async () => {
    let release: (outcome: Outcome<MirrorDto[]>) => void = () => {};
    const checkMirrors = vi.fn(
      () => new Promise<Outcome<MirrorDto[]>>((resolve) => (release = resolve)),
    );
    const store = createMirrorsStore(fakeApi({ checkMirrors }));

    const pending = store.getState().check();
    expect(store.getState().busy).toBe("checking");
    release({ ok: true, value: [mirror("https://kvk.pub/", { state: "open", latencyMs: 80, active: true })] });
    await pending;

    expect(checkMirrors).toHaveBeenCalledTimes(1);
    expect(store.getState()).toMatchObject({ busy: "idle", checked: true, error: null });
    expect(store.getState().mirrors[0]).toMatchObject({ state: "open", latencyMs: 80 });
  });

  it("не запускает вторую проверку, пока идёт первая", async () => {
    const checkMirrors = vi.fn(fakeApi().checkMirrors);
    const store = createMirrorsStore(fakeApi({ checkMirrors }));
    await Promise.all([store.getState().check(), store.getState().check()]);
    expect(checkMirrors).toHaveBeenCalledTimes(1);
  });

  it("ошибка проверки сохраняет текст и прежний список", async () => {
    const store = createMirrorsStore(
      fakeApi({ checkMirrors: async () => failure("internal", "сбой проверки") }),
    );
    await store.getState().load();
    await store.getState().check();
    expect(store.getState()).toMatchObject({ busy: "idle", error: "сбой проверки" });
    expect(store.getState().mirrors).toEqual(initial);
  });
});

describe("add", () => {
  it("невалидный адрес даёт ошибку и не меняет список", async () => {
    const addMirror = vi.fn(async () => failure("invalidInput", "Некорректный адрес зеркала"));
    const store = createMirrorsStore(fakeApi({ addMirror }));
    await store.getState().load();

    const ok = await store.getState().add("не адрес");

    expect(ok).toBe(false);
    expect(addMirror).toHaveBeenCalledWith("не адрес");
    expect(store.getState()).toMatchObject({ busy: "idle", error: "Некорректный адрес зеркала" });
    expect(store.getState().mirrors).toEqual(initial);
  });

  it("зеркало с проверкой на бота отклоняется с текстом ошибки", async () => {
    const store = createMirrorsStore(
      fakeApi({ addMirror: async () => failure("challenge", "Зеркало требует проверки") }),
    );
    expect(await store.getState().add("https://rezka.ag")).toBe(false);
    expect(store.getState().error).toBe("Зеркало требует проверки");
  });

  it("новый адрес добавляется в конец и становится активным", async () => {
    const store = createMirrorsStore(fakeApi());
    await store.getState().load();
    expect(await store.getState().add("https://new.example/")).toBe(true);

    const list = store.getState().mirrors;
    expect(list.map((item) => item.url)).toEqual([
      "https://kvk.pub/",
      "https://rezka.fi/",
      "https://new.example/",
    ]);
    expect(list.filter((item) => item.active).map((item) => item.url)).toEqual(["https://new.example/"]);
    expect(store.getState().error).toBeNull();
  });

  it("успех очищает прошлую ошибку", async () => {
    const addMirror = vi
      .fn<Api["addMirror"]>()
      .mockResolvedValueOnce(failure("invalidInput", "Некорректный адрес зеркала"))
      .mockResolvedValueOnce({ ok: true, value: mirror("https://ok.example/", { active: true }) });
    const store = createMirrorsStore(fakeApi({ addMirror }));
    await store.getState().add("x");
    await store.getState().add("https://ok.example");
    expect(store.getState().error).toBeNull();
  });
});

describe("select", () => {
  it("переносит отметку активного и сохраняет задержку проверки", async () => {
    const checked = [
      mirror("https://kvk.pub/", { state: "needsLogin", latencyMs: 90, active: true }),
      mirror("https://rezka.fi/", { state: "needsLogin", latencyMs: 150 }),
    ];
    const selectMirror = vi.fn(fakeApi().selectMirror);
    const store = createMirrorsStore(
      fakeApi({ checkMirrors: async () => ({ ok: true, value: checked }), selectMirror }),
    );
    await store.getState().check();
    await store.getState().select("https://rezka.fi/");

    expect(selectMirror).toHaveBeenCalledWith("https://rezka.fi/");
    const [first, second] = store.getState().mirrors;
    expect(first).toMatchObject({ url: "https://kvk.pub/", active: false, latencyMs: 90 });
    expect(second).toMatchObject({ url: "https://rezka.fi/", active: true, latencyMs: 150 });
  });

  it("отказ оставляет активным прежнее зеркало", async () => {
    const store = createMirrorsStore(
      fakeApi({ selectMirror: async () => failure("network", "Нет соединения") }),
    );
    await store.getState().load();
    expect(await store.getState().select("https://rezka.fi/")).toBe(false);
    expect(store.getState().mirrors.find((item) => item.active)?.url).toBe("https://kvk.pub/");
    expect(store.getState().error).toBe("Нет соединения");
  });
});

describe("mirrorHost", () => {
  it("убирает протокол и путь", () => {
    expect(mirrorHost("https://kvk.pub/")).toBe("kvk.pub");
  });

  it("не ломается на неразбираемом адресе", () => {
    expect(mirrorHost("не адрес")).toBe("не адрес");
  });
});
