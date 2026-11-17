import { create } from "zustand";

import type {
  Api,
  EpisodeDto,
  IpcError,
  MediaDetailsDto,
  MediaItemDto,
  StreamQualityDto,
  TranslatorId,
} from "./api";

const ERROR_DETAILS = "Не удалось загрузить данные. Попробуйте ещё раз.";
const ERROR_EPISODES = "Не удалось загрузить список серий. Попробуйте ещё раз.";
const ERROR_LAUNCH =
  "Не удалось запустить воспроизведение. Попробуйте другую озвучку, зеркало или VPN.";

// При `success:false` HDrezka возвращает конкретный текст (например про геоблок
// региона) — kino-api доносит его как `KinoError::Parse("<action>: <текст>")`.
// Технический префикс действия пользователю не нужен, важен сам текст сервера (#038).
const STREAMS_ACTION_PREFIX = /^(get_movie|get_stream|get_episodes):\s*/;
const GEOBLOCK_HINT =
  " Похоже, контент недоступен в вашем регионе — попробуйте другое зеркало или VPN.";
// Эвристика по ключевым словам: точный признак геоблока по одному тексту не
// выделить, но ложное срабатывание безвредно — подсказка про зеркало/VPN уместна
// при любой ошибке запуска.
const GEOBLOCK_KEYWORDS = ["регион", "region", "недоступ", "unavailable in your"];

function launchErrorMessage(error: IpcError): string {
  const text = error.message.replace(STREAMS_ACTION_PREFIX, "").trim();
  if (!text) return ERROR_LAUNCH;
  const lowered = text.toLowerCase();
  const looksLikeGeoblock = GEOBLOCK_KEYWORDS.some((keyword) => lowered.includes(keyword));
  return looksLikeGeoblock ? text + GEOBLOCK_HINT : text;
}

export interface DetailsState {
  open: boolean;
  item: MediaItemDto | null;
  details: MediaDetailsDto | null;
  loading: boolean;
  error: string | null;
  translatorId: TranslatorId | null;
  episodes: EpisodeDto[];
  episodesLoading: boolean;
  episodeIndex: number;
  launching: boolean;
  launchError: string | null;
  openFor: (item: MediaItemDto) => Promise<void>;
  close: () => void;
  selectTranslator: (id: TranslatorId) => void;
  selectEpisode: (index: number) => void;
  play: () => Promise<void>;
}

// Карточка — та же модалка для любого произведения: открытие сбрасывает всё, кроме `open`,
// а устаревшие ответы (клик по другому постеру до завершения предыдущей загрузки) отбрасываются
// по токену запроса, а не по сравнению `item`, потому что повторный клик по тому же постеру тоже
// должен перезапустить загрузку.
export function createDetailsStore(api: Api) {
  let requestId = 0;

  return create<DetailsState>()((set, get) => ({
    open: false,
    item: null,
    details: null,
    loading: false,
    error: null,
    translatorId: null,
    episodes: [],
    episodesLoading: false,
    episodeIndex: 0,
    launching: false,
    launchError: null,

    openFor: async (item) => {
      const id = ++requestId;
      set({
        open: true,
        item,
        details: null,
        loading: true,
        error: null,
        translatorId: null,
        episodes: [],
        episodesLoading: false,
        episodeIndex: 0,
        launching: false,
        launchError: null,
      });

      const outcome = await api.details(item);
      if (id !== requestId) return;
      if (!outcome.ok) {
        set({ loading: false, error: ERROR_DETAILS });
        return;
      }

      const details = outcome.value;
      const translatorId = details.translators[0]?.id ?? null;
      set({ details, loading: false, translatorId });

      if (details.seasons.length === 0) return;

      set({ episodesLoading: true });
      const episodesOutcome = await api.episodes(details);
      if (id !== requestId) return;
      if (episodesOutcome.ok) {
        set({ episodes: episodesOutcome.value, episodesLoading: false, episodeIndex: 0 });
      } else {
        set({ episodesLoading: false, error: ERROR_EPISODES });
      }
    },

    close: () => set({ open: false }),

    selectTranslator: (id) => set({ translatorId: id }),

    selectEpisode: (index) => {
      const count = get().episodes.length;
      if (index < 0 || index >= count) return;
      set({ episodeIndex: index });
    },

    play: async () => {
      const { item, details, translatorId, episodes, episodeIndex, launching } = get();
      if (!item || !details || launching) return;

      const isSeries = details.seasons.length > 0;
      const episode = isSeries ? (episodes[episodeIndex] ?? null) : null;
      if (isSeries && !episode) return;

      set({ launching: true, launchError: null });
      const streamsOutcome = await api.streams(item, translatorId, episode);
      if (!streamsOutcome.ok) {
        set({ launching: false, launchError: launchErrorMessage(streamsOutcome.error) });
        return;
      }

      const quality: StreamQualityDto | undefined = streamsOutcome.value[0];
      if (!quality) {
        set({ launching: false, launchError: ERROR_LAUNCH });
        return;
      }

      const loadOutcome = await api.playerLoad(quality, streamsOutcome.value);
      if (!loadOutcome.ok) {
        set({ launching: false, launchError: ERROR_LAUNCH });
        return;
      }

      set({ launching: false, open: false });
    },
  }));
}
