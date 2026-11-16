import { getCurrentWindow } from "@tauri-apps/api/window";
import type { MouseEvent } from "react";

import type { StreamQualityDto } from "../api";
import { formatTime } from "../time-format";
import { useAutoHide } from "../use-auto-hide";

export interface PlayerControlsViewProps {
  qualities: StreamQualityDto[];
  qualitiesLoading: boolean;
  currentQuality: StreamQualityDto | null;
  position: number | null; // секунды, null пока неизвестно
  duration: number | null; // секунды, null пока неизвестно
  paused: boolean;
  volume: number; // 0..1
  error: string | null;
  onPlay: () => void;
  onPause: () => void;
  onSeek: (seconds: number) => void; // абсолютная позиция в секундах
  onSelectQuality: (quality: StreamQualityDto) => void;
  onSetVolume: (value: number) => void; // 0..1
  onClose: () => void;
}

const HIDE_DELAY_MS = 3000;

export function PlayerControlsView(props: PlayerControlsViewProps) {
  const {
    qualities,
    qualitiesLoading,
    currentQuality,
    position,
    duration,
    paused,
    volume,
    error,
    onPlay,
    onPause,
    onSeek,
    onSelectQuality,
    onSetVolume,
    onClose,
  } = props;

  const visible = useAutoHide(HIDE_DELAY_MS);

  const progress = computeProgress(position, duration);

  const handleSeek = (event: MouseEvent<HTMLDivElement>) => {
    if (duration === null || duration <= 0) {
      return;
    }
    const rect = event.currentTarget.getBoundingClientRect();
    const ratio = (event.clientX - rect.left) / rect.width;
    const clamped = Math.min(1, Math.max(0, ratio));
    onSeek(clamped * duration);
  };

  return (
    <div className="pointer-events-none fixed inset-0 select-none">
      {/* Полупрозрачная подложка снизу, типичная для видео-UI. Вся верхняя часть
          окна остаётся прозрачной и не перехватывает события. */}
      <div className="absolute inset-0 bg-gradient-to-t from-black/80 via-black/30 to-transparent" />

      {/* Блок реальных контролов. CSS `pointer-events-auto` решает, долетит ли
          событие до DOM; но окно `player-controls` прозрачно лежит ПОВЕРХ окна
          `player`, поэтому чтобы клики вне этого блока проходили насквозь к видео,
          дополнительно включаем/выключаем игнор событий на уровне окна через
          нативный `setIgnoreCursorEvents` (CSS `pointer-events` этого не умеет). */}
      <div
        className={`pointer-events-auto absolute inset-x-0 bottom-0 flex flex-col gap-3 px-6 pb-4 pt-16 transition-opacity duration-300 ${
          visible ? "opacity-100" : "opacity-0"
        }`}
        onMouseEnter={() => void getCurrentWindow().setIgnoreCursorEvents(false)}
        onMouseLeave={() => void getCurrentWindow().setIgnoreCursorEvents(true)}
      >
        {error !== null && (
          <p role="alert" className="text-card text-accent">
            {error}
          </p>
        )}

        <div className="flex items-center gap-3">
          <button
            type="button"
            onClick={paused ? onPlay : onPause}
            aria-label={paused ? "Воспроизвести" : "Пауза"}
            aria-pressed={!paused}
            className="flex h-12 w-12 shrink-0 items-center justify-center rounded-button bg-white/10 text-card text-text outline-none hover:bg-white/15 focus-visible:ring-2 focus-visible:ring-accent/70"
          >
            {paused ? "▶" : "❚❚"}
          </button>

          <div className="flex min-w-0 flex-1 items-center gap-3">
            <span className="w-12 shrink-0 text-right text-card tabular-nums text-text">
              {formatTime(position)}
            </span>

            {/* Прогресс-бар: клик перематывает к абсолютной позиции под курсором. */}
            <div
              role="slider"
              aria-label="Перемотка"
              aria-valuemin={0}
              aria-valuemax={duration ?? 0}
              aria-valuenow={Math.round(position ?? 0)}
              aria-valuetext={`${formatTime(position)} из ${formatTime(duration)}`}
              onClick={handleSeek}
              className="group relative h-6 flex-1 cursor-pointer"
            >
              <div className="absolute inset-x-0 top-1/2 h-1 -translate-y-1/2 rounded-full bg-white/20">
                <div
                  className="h-full rounded-full bg-accent"
                  style={{ width: `${progress}%` }}
                />
              </div>
              <div
                className="absolute top-1/2 h-3 w-3 -translate-x-1/2 -translate-y-1/2 rounded-full bg-accent opacity-0 transition-opacity group-hover:opacity-100"
                style={{ left: `${progress}%` }}
              />
            </div>

            <span className="w-12 shrink-0 text-card tabular-nums text-text-2">
              {formatTime(duration)}
            </span>
          </div>

          <VolumeControl volume={volume} onSetVolume={onSetVolume} />

          <QualityPicker
            qualities={qualities}
            qualitiesLoading={qualitiesLoading}
            currentQuality={currentQuality}
            onSelectQuality={onSelectQuality}
          />

          <button
            type="button"
            onClick={onClose}
            aria-label="Закрыть плеер"
            className="flex h-12 w-12 shrink-0 items-center justify-center rounded-button bg-white/10 text-card text-text outline-none hover:bg-white/15 focus-visible:ring-2 focus-visible:ring-accent/70"
          >
            ×
          </button>
        </div>
      </div>
    </div>
  );
}

function computeProgress(position: number | null, duration: number | null): number {
  if (position === null || duration === null || duration <= 0) {
    return 0;
  }
  return Math.min(100, Math.max(0, (position / duration) * 100));
}

function VolumeControl({
  volume,
  onSetVolume,
}: {
  volume: number;
  onSetVolume: (value: number) => void;
}) {
  return (
    <label className="flex shrink-0 items-center gap-2 text-card text-text-2">
      <span aria-hidden="true">🔊</span>
      <input
        type="range"
        min={0}
        max={1}
        step={0.01}
        value={volume}
        aria-label="Громкость"
        onChange={(event) => onSetVolume(Number(event.target.value))}
        className="h-1 w-24 cursor-pointer accent-accent"
      />
    </label>
  );
}

function QualityPicker({
  qualities,
  qualitiesLoading,
  currentQuality,
  onSelectQuality,
}: {
  qualities: StreamQualityDto[];
  qualitiesLoading: boolean;
  currentQuality: StreamQualityDto | null;
  onSelectQuality: (quality: StreamQualityDto) => void;
}) {
  if (qualitiesLoading) {
    return (
      <div className="flex shrink-0 items-center gap-2 px-3 text-card text-text-2">
        <Spinner />
      </div>
    );
  }
  if (qualities.length === 0) {
    return null;
  }

  const currentLabel = currentQuality?.label ?? qualities[0]?.label;

  return (
    <select
      aria-label="Качество"
      value={currentLabel}
      onChange={(event) => {
        const quality = qualities.find((q) => q.label === event.target.value);
        if (quality) {
          onSelectQuality(quality);
        }
      }}
      className="h-12 shrink-0 cursor-pointer rounded-button bg-white/10 px-3 text-card text-text outline-none hover:bg-white/15 focus-visible:ring-2 focus-visible:ring-accent/70"
    >
      {qualities.map((quality) => (
        <option key={quality.url} value={quality.label} className="bg-bg text-text">
          {quality.label}
        </option>
      ))}
    </select>
  );
}

function Spinner() {
  return (
    <svg aria-hidden="true" viewBox="0 0 24 24" className="h-3.5 w-3.5 animate-spin" fill="none">
      <circle cx="12" cy="12" r="9" stroke="currentColor" strokeOpacity="0.25" strokeWidth="3" />
      <path d="M21 12a9 9 0 0 0-9-9" stroke="currentColor" strokeWidth="3" strokeLinecap="round" />
    </svg>
  );
}
