import { create } from "zustand";

import type { Api, MirrorDto, Outcome } from "./api";

export type MirrorsBusy = "idle" | "checking" | "switching";

// Отдельно от session-store: здесь нет ни пароля, ни фазы входа, только список зеркал и ошибка последней операции.
export interface MirrorsState {
  mirrors: MirrorDto[];
  checked: boolean;
  busy: MirrorsBusy;
  error: string | null;
  load: () => Promise<void>;
  check: () => Promise<void>;
  add: (url: string) => Promise<boolean>;
  select: (url: string) => Promise<boolean>;
}

// Бэкенд сравнивает схему, хост и порт; `origin` убирает порт по умолчанию и путь так же.
export function mirrorKey(url: string): string {
  try {
    return new URL(url).origin;
  } catch {
    return url;
  }
}

export function mirrorHost(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}

function withActivated(list: MirrorDto[], resolved: MirrorDto): MirrorDto[] {
  const key = mirrorKey(resolved.url);
  let found = false;
  const next = list.map((mirror) => {
    if (mirrorKey(mirror.url) === key) {
      found = true;
      return { ...resolved, latencyMs: mirror.latencyMs, standby: mirror.standby, active: true };
    }
    return mirror.active ? { ...mirror, active: false } : mirror;
  });
  return found ? next : [...next, { ...resolved, active: true }];
}

// Свежий список (без сети) задаёт состав и активное зеркало, результаты прошлой проверки переносятся по адресу.
function withPreviousChecks(fresh: MirrorDto[], previous: MirrorDto[]): MirrorDto[] {
  const known = new Map(previous.map((mirror) => [mirrorKey(mirror.url), mirror]));
  const merged = fresh.map((mirror) => {
    const old = known.get(mirrorKey(mirror.url));
    return old
      ? { ...mirror, state: old.state, reason: old.reason, latencyMs: old.latencyMs, standby: old.standby }
      : mirror;
  });
  const freshKeys = new Set(fresh.map((mirror) => mirrorKey(mirror.url)));
  const extras = previous
    .filter((mirror) => !freshKeys.has(mirrorKey(mirror.url)))
    .map((mirror) => ({ ...mirror, active: false }));
  return [...merged, ...extras];
}

export function createMirrorsStore(api: Api) {
  return create<MirrorsState>()((set, get) => {
    const activate = async (request: Promise<Outcome<MirrorDto>>): Promise<boolean> => {
      const outcome = await request;
      if (outcome.ok) {
        set({
          mirrors: withActivated(get().mirrors, outcome.value),
          busy: "idle",
          error: null,
        });
        return true;
      }
      set({ busy: "idle", error: outcome.error.message });
      return false;
    };

    return {
      mirrors: [],
      checked: false,
      busy: "idle",
      error: null,

      // Без сети: безопасно вызывать при каждом показе окна.
      load: async () => {
        const outcome = await api.mirrors();
        if (outcome.ok) {
          set({ mirrors: withPreviousChecks(outcome.value, get().mirrors) });
        } else {
          set({ error: outcome.error.message });
        }
      },

      // Единственное место, где UI ходит в сеть за состоянием зеркал: только по явному действию пользователя.
      check: async () => {
        if (get().busy !== "idle") {
          return;
        }
        set({ busy: "checking", checked: true, error: null });
        const outcome = await api.checkMirrors();
        if (outcome.ok) {
          set({ mirrors: outcome.value, busy: "idle" });
        } else {
          set({ busy: "idle", error: outcome.error.message });
        }
      },

      add: async (url) => {
        if (get().busy !== "idle") {
          return false;
        }
        set({ busy: "switching", error: null });
        return activate(api.addMirror(url));
      },

      select: async (url) => {
        if (get().busy !== "idle") {
          return false;
        }
        set({ busy: "switching", error: null });
        return activate(api.selectMirror(url));
      },
    };
  });
}
