import { describe, expect, it, vi } from "vitest";

import type { Api, BootState, IpcError, Outcome, UserDto } from "./api";
import { createSessionStore } from "./session-store";

const user: UserDto = { userId: 42, username: "kino" };

function fakeApi(overrides: Partial<Api> = {}): Api {
  return {
    boot: async () => ({ ok: true, value: { state: "needsLogin", email: null } }),
    login: async () => ({ ok: true, value: user }),
    logout: async () => ({ ok: true, value: null }),
    session: async () => user,
    mirrors: async () => ({ ok: true, value: [] }),
    checkMirrors: async () => ({ ok: true, value: [] }),
    addMirror: async () => ({ ok: false, error: { kind: "internal", message: "не вызывается" } }),
    selectMirror: async () => ({ ok: false, error: { kind: "internal", message: "не вызывается" } }),
    catalog: async () => ({ ok: true, value: [] }),
    search: async () => ({ ok: true, value: [] }),
    quit: async () => {},
    ...overrides,
  };
}

function bootsTo(state: BootState): Api["boot"] {
  return async () => ({ ok: true, value: state });
}

describe("boot", () => {
  it("needsLogin подставляет сохранённый email", async () => {
    const store = createSessionStore(
      fakeApi({ boot: bootsTo({ state: "needsLogin", email: "a@b.c" }) }),
    );
    await store.getState().boot();
    expect(store.getState()).toMatchObject({ phase: "needsLogin", email: "a@b.c" });
  });

  it("signedIn сохраняет пользователя", async () => {
    const store = createSessionStore(fakeApi({ boot: bootsTo({ state: "signedIn", user }) }));
    await store.getState().boot();
    expect(store.getState()).toMatchObject({ phase: "signedIn", user });
  });

  it("unavailable сохраняет сообщение", async () => {
    const store = createSessionStore(
      fakeApi({ boot: bootsTo({ state: "unavailable", message: "нет сети" }) }),
    );
    await store.getState().boot();
    expect(store.getState()).toMatchObject({ phase: "unavailable", message: "нет сети" });
  });

  it("ошибка команды переводит в unavailable", async () => {
    const error: IpcError = { kind: "internal", message: "сбой" };
    const store = createSessionStore(fakeApi({ boot: async () => ({ ok: false, error }) }));
    await store.getState().boot();
    expect(store.getState()).toMatchObject({ phase: "unavailable", message: "сбой" });
  });

  it("повторный вызов во время запроса не делает второй boot", async () => {
    const boot = vi.fn(bootsTo({ state: "needsLogin", email: null }));
    const store = createSessionStore(fakeApi({ boot }));
    await Promise.all([store.getState().boot(), store.getState().boot()]);
    expect(boot).toHaveBeenCalledTimes(1);
  });

  it("повтор после unavailable снова вызывает boot", async () => {
    const boot = vi
      .fn<Api["boot"]>()
      .mockResolvedValueOnce({ ok: true, value: { state: "unavailable", message: "нет сети" } })
      .mockResolvedValueOnce({ ok: true, value: { state: "signedIn", user } });
    const store = createSessionStore(fakeApi({ boot }));
    await store.getState().boot();
    await store.getState().boot();
    expect(store.getState().phase).toBe("signedIn");
  });
});

describe("login", () => {
  it("успех переводит в signedIn", async () => {
    const store = createSessionStore(fakeApi());
    await store.getState().login("a@b.c", "secret");
    expect(store.getState()).toMatchObject({ phase: "signedIn", user, submitting: false });
  });

  it("отказ сохраняет текст ошибки и не меняет фазу", async () => {
    const error: IpcError = { kind: "loginRejected", message: "Неверный пароль" };
    const login = vi.fn(async (): Promise<Outcome<UserDto>> => ({ ok: false, error }));
    const store = createSessionStore(fakeApi({ login }));
    await store.getState().boot();
    await store.getState().login("a@b.c", "secret");
    expect(store.getState()).toMatchObject({
      phase: "needsLogin",
      error: "Неверный пароль",
      submitting: false,
    });
  });

  it("пароль не попадает в состояние стора", async () => {
    const store = createSessionStore(fakeApi());
    await store.getState().login("a@b.c", "secret-password");
    expect(JSON.stringify(store.getState())).not.toContain("secret-password");
  });
});
