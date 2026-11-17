import { useEffect, useMemo, useState } from "react";

import type {
  EpisodeDto,
  MediaDetailsDto,
  MediaItemDto,
  TranslatorId,
} from "../api";

export interface DetailsViewProps {
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
  onClose: () => void;
  onSelectTranslator: (id: TranslatorId) => void;
  onSelectEpisode: (index: number) => void;
  onPlay: () => void;
}

export function DetailsView({
  item,
  details,
  loading,
  error,
  translatorId,
  episodes,
  episodesLoading,
  episodeIndex,
  launching,
  launchError,
  onClose,
  onSelectTranslator,
  onSelectEpisode,
  onPlay,
}: DetailsViewProps) {
  const isSeries = (details?.seasons.length ?? 0) > 0;
  const hasEpisode = !isSeries || episodes.length > 0;

  // Выбранный сезон выводим как табы; выбранная серия подсвечивается по episodeIndex.
  const [seasonNumber, setSeasonNumber] = useState<number | null>(null);
  const activeSeason = useMemo(
    () => details?.seasons.find((season) => season.number === seasonNumber) ?? null,
    [details, seasonNumber],
  );
  useEffect(() => {
    const first = details?.seasons[0]?.number ?? null;
    setSeasonNumber((current) => current ?? first);
  }, [details]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        onClose();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  const seasonEpisodes = useMemo(
    () =>
      activeSeason === null
        ? []
        : episodes.filter((episode) => episode.seasonId === activeSeason.number),
    [episodes, activeSeason],
  );

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label={item?.title ?? "Карточка деталей"}
      onClick={onClose}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/80 p-6"
    >
      <div
        onClick={(event) => event.stopPropagation()}
        className="flex max-h-full w-full max-w-3xl flex-col gap-6 overflow-hidden rounded-modal bg-bg p-6"
      >
        {loading ? (
          <div className="flex items-center justify-center gap-3 py-16 text-card text-text-2">
            <Spinner />
            Загрузка…
          </div>
        ) : error || !details || !item ? (
          <p role="alert" className="py-16 text-center text-card text-accent">
            {error ?? "Не удалось загрузить данные. Попробуйте ещё раз."}
          </p>
        ) : (
          <div className="flex min-h-0 flex-col gap-6 overflow-y-auto">
            <div className="relative shrink-0 overflow-hidden rounded-modal bg-surface">
              <div className="relative aspect-video w-full">
                {item.posterUrl ? (
                  <img
                    src={item.posterUrl}
                    alt={item.title}
                    draggable={false}
                    className="h-full w-full object-cover"
                  />
                ) : (
                  <div className="flex h-full w-full items-center justify-center bg-surface">
                    <span className="px-8 text-center text-section text-text-2">
                      {item.title}
                    </span>
                  </div>
                )}
                <div className="absolute inset-0 bg-gradient-to-t from-black/90 via-black/40 to-transparent" />
                <div className="absolute inset-x-0 bottom-0 flex flex-col gap-2 p-8">
                  <h2 className="text-hero text-text">{item.title}</h2>
                  <p className="text-card text-text-2">
                    {[item.year, item.rating !== null ? `★ ${item.rating}` : null]
                      .filter((part) => part !== null)
                      .join(" · ")}
                  </p>
                </div>
              </div>
            </div>

            <div className="flex flex-col gap-4">
              {details.description && (
                <p className="text-card text-text">{details.description}</p>
              )}

              {(details.genres.length > 0 || details.countries.length > 0) && (
                <div className="flex flex-col gap-1 text-card text-text-2">
                  {details.genres.length > 0 && <p>{details.genres.join(", ")}</p>}
                  {details.countries.length > 0 && (
                    <p>{details.countries.join(", ")}</p>
                  )}
                </div>
              )}

              {details.durationMinutes !== null && (
                <p className="text-card text-text-2">
                  {formatDuration(details.durationMinutes)}
                </p>
              )}

              {details.translators.length > 0 && (
                <div
                  role="group"
                  aria-label="Перевод"
                  className="flex flex-wrap gap-2"
                >
                  {details.translators.map((translator) => (
                    <button
                      key={translator.id}
                      type="button"
                      onClick={() => onSelectTranslator(translator.id as TranslatorId)}
                      aria-pressed={translator.id === translatorId}
                      className={`rounded-button px-3 py-1.5 text-card outline-none focus-visible:ring-2 focus-visible:ring-accent/70 ${
                        translator.id === translatorId
                          ? "bg-accent text-white"
                          : "bg-white/10 text-text hover:bg-white/15"
                      }`}
                    >
                      {translator.name}
                      {translator.premium && (
                        <span
                          title="Платная премиум-озвучка: без подписки вместо потока может проиграться напоминающий ролик"
                          className="ml-1.5 rounded bg-premium/20 px-1 py-0.5 text-[0.65rem] font-semibold uppercase tracking-wide text-premium"
                        >
                          Premium
                        </span>
                      )}
                    </button>
                  ))}
                </div>
              )}

              {isSeries && (
                <div className="flex flex-col gap-2">
                  <div role="tablist" aria-label="Сезоны" className="flex flex-wrap gap-2">
                    {details.seasons.map((season) => (
                      <button
                        key={season.number}
                        type="button"
                        role="tab"
                        aria-selected={season.number === activeSeason?.number}
                        onClick={() => setSeasonNumber(season.number)}
                        className={`rounded-button px-3 py-1.5 text-card outline-none focus-visible:ring-2 focus-visible:ring-accent/70 ${
                          season.number === activeSeason?.number
                            ? "bg-white/15 text-text"
                            : "bg-white/10 text-text-2 hover:bg-white/15"
                        }`}
                      >
                        Сезон {season.number}
                      </button>
                    ))}
                  </div>

                  {episodesLoading ? (
                    <div className="flex items-center gap-3 py-4 text-card text-text-2">
                      <Spinner />
                      Загрузка серий…
                    </div>
                  ) : (
                    <ul className="flex flex-col gap-1">
                      {seasonEpisodes.map((episode) => (
                        <li key={`${episode.seasonId}-${episode.episodeId}`}>
                          <button
                            type="button"
                            onClick={() => onSelectEpisode(episodes.indexOf(episode))}
                            aria-current={
                              episodes.indexOf(episode) === episodeIndex
                                ? "true"
                                : undefined
                            }
                            className={`flex h-8 w-full items-center gap-3 rounded-button px-3 text-left text-card outline-none focus-visible:ring-2 focus-visible:ring-accent/70 ${
                              episodes.indexOf(episode) === episodeIndex
                                ? "bg-white/15"
                                : "hover:bg-white/5"
                            }`}
                          >
                            <span className="text-text-2">{episode.episodeId}.</span>
                            <span className="min-w-0 flex-1 truncate text-text">
                              {episode.title}
                            </span>
                          </button>
                        </li>
                      ))}
                    </ul>
                  )}
                </div>
              )}
            </div>

            <div className="flex shrink-0 flex-col gap-2">
              <button
                type="button"
                onClick={onPlay}
                disabled={launching || !hasEpisode}
                className="flex h-12 items-center justify-center gap-2 rounded-button bg-accent text-card font-semibold text-text outline-none hover:brightness-110 focus-visible:ring-2 focus-visible:ring-accent/70 disabled:opacity-40"
              >
                {launching ? (
                  <>
                    <Spinner />
                    Запуск…
                  </>
                ) : (
                  "▶ Смотреть"
                )}
              </button>
              <p role="alert" className="min-h-5 text-card text-accent">
                {launchError}
              </p>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function formatDuration(minutes: number): string {
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  if (hours === 0) {
    return `${rest} мин`;
  }
  return rest === 0 ? `${hours} ч` : `${hours} ч ${rest} мин`;
}

function Spinner() {
  return (
    <svg aria-hidden="true" viewBox="0 0 24 24" className="h-3.5 w-3.5 animate-spin" fill="none">
      <circle cx="12" cy="12" r="9" stroke="currentColor" strokeOpacity="0.25" strokeWidth="3" />
      <path d="M21 12a9 9 0 0 0-9-9" stroke="currentColor" strokeWidth="3" strokeLinecap="round" />
    </svg>
  );
}
