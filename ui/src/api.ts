// Единственное место, где UI знает о Tauri и сгенерированных привязках.
import {
  commands,
  type BootState,
  type EpisodeDto,
  type IpcError,
  type MediaDetailsDto,
  type MediaItemDto,
  type MediaKindDto,
  type MirrorDto,
  type MirrorStateDto,
  type PlayerProgressDto,
  type StreamQualityDto,
  type TranslatorId,
  type UserDto,
} from "./bindings";

export type {
  BootState,
  EpisodeDto,
  IpcError,
  MediaDetailsDto,
  MediaItemDto,
  MediaKindDto,
  MirrorDto,
  MirrorStateDto,
  PlayerProgressDto,
  StreamQualityDto,
  TranslatorId,
  UserDto,
};

export type Outcome<T> = { ok: true; value: T } | { ok: false; error: IpcError };

export interface Api {
  boot(): Promise<Outcome<BootState>>;
  login(email: string, password: string): Promise<Outcome<UserDto>>;
  logout(): Promise<Outcome<null>>;
  session(): Promise<UserDto | null>;
  mirrors(): Promise<Outcome<MirrorDto[]>>;
  checkMirrors(): Promise<Outcome<MirrorDto[]>>;
  addMirror(url: string): Promise<Outcome<MirrorDto>>;
  selectMirror(url: string): Promise<Outcome<MirrorDto>>;
  catalog(kind: MediaKindDto | null, page: number): Promise<Outcome<MediaItemDto[]>>;
  search(query: string): Promise<Outcome<MediaItemDto[]>>;
  details(item: MediaItemDto): Promise<Outcome<MediaDetailsDto>>;
  episodes(details: MediaDetailsDto): Promise<Outcome<EpisodeDto[]>>;
  streams(
    item: MediaItemDto,
    translatorId: TranslatorId | null,
    episode: EpisodeDto | null,
  ): Promise<Outcome<StreamQualityDto[]>>;
  playerLoad(quality: StreamQualityDto, qualities: StreamQualityDto[]): Promise<Outcome<null>>;
  playerQualities(): Promise<Outcome<StreamQualityDto[]>>;
  playerPlay(): Promise<Outcome<null>>;
  playerPause(): Promise<Outcome<null>>;
  playerSeek(seconds: number): Promise<Outcome<null>>;
  playerSetQuality(quality: StreamQualityDto): Promise<Outcome<null>>;
  playerSetVolume(volume: number): Promise<Outcome<null>>;
  playerStop(): Promise<Outcome<null>>;
  quit(): Promise<void>;
}

const UNKNOWN_ERROR: IpcError = {
  kind: "internal",
  message: "Не удалось выполнить запрос. Попробуйте ещё раз.",
};

function isIpcError(value: unknown): value is IpcError {
  return (
    typeof value === "object" &&
    value !== null &&
    "kind" in value &&
    "message" in value &&
    typeof value.message === "string"
  );
}

// Сбои самого Tauri приходят строкой или Error. Их текст не показываем и не логируем.
async function call<T>(
  run: () => Promise<{ status: "ok"; data: T } | { status: "error"; error: IpcError }>,
): Promise<Outcome<T>> {
  try {
    const result = await run();
    if (result.status === "ok") {
      return { ok: true, value: result.data };
    }
    return { ok: false, error: isIpcError(result.error) ? result.error : UNKNOWN_ERROR };
  } catch {
    return { ok: false, error: UNKNOWN_ERROR };
  }
}

export const tauriApi: Api = {
  boot: () => call(() => commands.boot()),
  login: (email, password) => call(() => commands.login(email, password)),
  logout: () => call(() => commands.logout()),
  session: async () => {
    try {
      return await commands.session();
    } catch {
      return null;
    }
  },
  mirrors: () => call(() => commands.mirrors()),
  checkMirrors: () => call(() => commands.checkMirrors()),
  addMirror: (url) => call(() => commands.addMirror(url)),
  selectMirror: (url) => call(() => commands.selectMirror(url)),
  catalog: (kind, page) => call(() => commands.catalog(kind, page)),
  search: (query) => call(() => commands.search(query)),
  details: (item) => call(() => commands.details(item)),
  episodes: (details) => call(() => commands.episodes(details)),
  streams: (item, translatorId, episode) => call(() => commands.streams(item, translatorId, episode)),
  playerLoad: (quality, qualities) => call(() => commands.playerLoad(quality, qualities)),
  playerQualities: () => call(() => commands.playerQualities()),
  playerPlay: () => call(() => commands.playerPlay()),
  playerPause: () => call(() => commands.playerPause()),
  playerSeek: (seconds) => call(() => commands.playerSeek(seconds)),
  playerSetQuality: (quality) => call(() => commands.playerSetQuality(quality)),
  playerSetVolume: (volume) => call(() => commands.playerSetVolume(volume)),
  playerStop: () => call(() => commands.playerStop()),
  quit: async () => {
    try {
      await commands.quit();
    } catch {
      // Приложение завершается, ответа может не быть.
    }
  },
};
