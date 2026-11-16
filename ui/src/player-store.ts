import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";

import type { Api, PlayerProgressDto, StreamQualityDto } from "./api";

const ERROR_QUALITIES = "Не удалось получить список качеств.";
const ERROR_ACTION = "Не удалось выполнить действие плеера. Попробуйте ещё раз.";

export interface PlayerControlsState {
  qualities: StreamQualityDto[];
  qualitiesLoading: boolean;
  currentQuality: StreamQualityDto | null;
  position: number | null;
  duration: number | null;
  paused: boolean;
  volume: number;
  error: string | null;
  init: () => Promise<void>;
  dispose: () => void;
  play: () => Promise<void>;
  pause: () => Promise<void>;
  seek: (seconds: number) => Promise<void>;
  setQuality: (quality: StreamQualityDto) => Promise<void>;
  setVolume: (volume: number) => Promise<void>;
  close: () => Promise<void>;
}

// Окно `player-controls` — отдельная JS-среда без доступа к стору главного окна: список качеств
// приходит отдельной командой (`playerQualities`), а не через zustand-состояние другого окна.
// Прогресс (`position`/`duration`/`paused`) приходит не командой, а сырым событием Tauri
// (`player-progress`, см. `start_progress_thread` в `src-tauri`) — единственное место в `ui/`,
// где стор подписывается на событие, а не только вызывает команды.
export function createPlayerStore(api: Api) {
  let unlisten: (() => void) | null = null;
  let initInFlight: Promise<void> | null = null;

  return create<PlayerControlsState>()((_set, get) => {
    const set = _set;
    return {
      qualities: [],
      qualitiesLoading: false,
      currentQuality: null,
      position: null,
      duration: null,
      paused: false,
      volume: 1,
      error: null,

      // Идемпотентен: повторный вызов (ремаунт контейнера) не плодит вторую подписку на событие.
      init: () => {
        if (initInFlight) return initInFlight;
        initInFlight = (async () => {
          set({ qualitiesLoading: true, error: null });
          const outcome = await api.playerQualities();
          if (outcome.ok) {
            set({
              qualities: outcome.value,
              currentQuality: outcome.value[0] ?? null,
              qualitiesLoading: false,
            });
          } else {
            set({ qualitiesLoading: false, error: ERROR_QUALITIES });
          }

          if (!unlisten) {
            unlisten = await listen<PlayerProgressDto>("player-progress", (event) => {
              set({
                position: event.payload.position,
                duration: event.payload.duration,
                paused: event.payload.paused,
              });
            });
          }
        })();
        return initInFlight;
      },

      dispose: () => {
        unlisten?.();
        unlisten = null;
        initInFlight = null;
      },

      play: async () => {
        const outcome = await api.playerPlay();
        if (!outcome.ok) set({ error: ERROR_ACTION });
      },

      pause: async () => {
        const outcome = await api.playerPause();
        if (!outcome.ok) set({ error: ERROR_ACTION });
      },

      seek: async (seconds) => {
        const outcome = await api.playerSeek(seconds);
        if (!outcome.ok) set({ error: ERROR_ACTION });
      },

      setQuality: async (quality) => {
        const outcome = await api.playerSetQuality(quality);
        if (outcome.ok) {
          set({ currentQuality: quality });
        } else {
          set({ error: ERROR_ACTION });
        }
      },

      setVolume: async (volume) => {
        set({ volume });
        const outcome = await api.playerSetVolume(volume);
        if (!outcome.ok) set({ error: ERROR_ACTION });
      },

      // Закрывает подписку на событие до ответа команды — после `playerStop` окно `player`
      // (и вместе с ним `player-controls`) закрывается Rust-стороной, слушать уже нечего.
      close: async () => {
        get().dispose();
        const outcome = await api.playerStop();
        if (!outcome.ok) set({ error: ERROR_ACTION });
      },
    };
  });
}
