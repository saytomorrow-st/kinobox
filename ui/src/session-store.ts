import { create } from "zustand";

import type { Api, UserDto } from "./api";

export type Phase = "idle" | "booting" | "needsLogin" | "signedIn" | "unavailable";

// Пароля в сторе нет: он живёт только в локальном состоянии формы и уходит аргументом `login`.
export interface SessionState {
  phase: Phase;
  user: UserDto | null;
  email: string | null;
  message: string | null;
  error: string | null;
  submitting: boolean;
  boot: () => Promise<void>;
  login: (email: string, password: string) => Promise<void>;
  logout: () => Promise<void>;
  loadSession: () => Promise<void>;
  quit: () => Promise<void>;
}

export function createSessionStore(api: Api) {
  let inflightBoot: Promise<void> | null = null;

  return create<SessionState>()((set, get) => ({
    phase: "idle",
    user: null,
    email: null,
    message: null,
    error: null,
    submitting: false,

    boot: () => {
      if (inflightBoot) {
        return inflightBoot;
      }
      set({ phase: "booting", error: null, message: null });
      inflightBoot = api
        .boot()
        .then((outcome) => {
          if (!outcome.ok) {
            set({ phase: "unavailable", message: outcome.error.message });
            return;
          }
          const state = outcome.value;
          switch (state.state) {
            case "needsLogin":
              set({ phase: "needsLogin", email: state.email });
              break;
            case "signedIn":
              set({ phase: "signedIn", user: state.user });
              break;
            case "unavailable":
              set({ phase: "unavailable", message: state.message });
              break;
          }
        })
        .finally(() => {
          inflightBoot = null;
        });
      return inflightBoot;
    },

    login: async (email, password) => {
      if (get().submitting) {
        return;
      }
      set({ submitting: true, error: null });
      const outcome = await api.login(email, password);
      if (outcome.ok) {
        set({ phase: "signedIn", user: outcome.value, submitting: false });
      } else {
        set({ error: outcome.error.message, submitting: false });
      }
    },

    logout: async () => {
      set({ error: null });
      const outcome = await api.logout();
      if (!outcome.ok) {
        set({ error: outcome.error.message });
      }
    },

    loadSession: async () => {
      const user = await api.session();
      set({ user, phase: user ? "signedIn" : "idle" });
    },

    quit: () => api.quit(),
  }));
}
