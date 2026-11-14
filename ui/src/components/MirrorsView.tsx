import { useState, type FormEvent } from "react";

import type { MirrorDto, MirrorStateDto } from "../api";
import { mirrorHost, type MirrorsBusy } from "../mirrors-store";

interface StateInfo {
  dot: string;
  label: string;
}

const STATE_INFO: Record<MirrorStateDto, StateInfo> = {
  open: { dot: "bg-status-open", label: "Доступно" },
  needsLogin: { dot: "bg-status-login", label: "Нужен вход" },
  challenge: { dot: "bg-status-challenge", label: "Проверка на бота" },
  dead: { dot: "bg-status-dead", label: "Недоступно" },
  unknown: { dot: "bg-status-unknown", label: "Не проверено" },
};

export interface MirrorsViewProps {
  open: boolean;
  onToggle: () => void;
  mirrors: MirrorDto[];
  checked: boolean;
  busy: MirrorsBusy;
  error: string | null;
  onCheck: () => void;
  onSelect: (url: string) => void;
  onAdd: (url: string) => Promise<boolean>;
}

// Адрес без схемы («kvk.pub») бэкенд не принимает, поэтому дописываем https:// до отправки.
export function normalizeMirrorInput(raw: string): string {
  const value = raw.trim();
  return value === "" || value.includes("://") ? value : `https://${value}`;
}

export function MirrorsView({
  open,
  onToggle,
  mirrors,
  checked,
  busy,
  error,
  onCheck,
  onSelect,
  onAdd,
}: MirrorsViewProps) {
  const [address, setAddress] = useState("");
  const idle = busy === "idle";
  const activeHost = mirrors.find((mirror) => mirror.active);

  const onSubmit = async (event: FormEvent) => {
    event.preventDefault();
    if (!idle || address.trim() === "") {
      return;
    }
    if (await onAdd(normalizeMirrorInput(address))) {
      setAddress("");
    }
  };

  return (
    <section aria-label="Зеркала" className="flex min-h-0 flex-col">
      <button
        type="button"
        onClick={onToggle}
        aria-expanded={open}
        aria-controls="mirrors-panel"
        className="flex h-7 items-center gap-2 self-start rounded-button pr-2 text-card text-text-2 outline-none hover:text-text focus-visible:ring-2 focus-visible:ring-accent/70"
      >
        <span
          aria-hidden="true"
          className={`inline-block w-3 text-center transition-transform ${open ? "rotate-90" : ""}`}
        >
          ›
        </span>
        <span>Зеркала</span>
        {!open && activeHost && <span className="text-text-2/70">{mirrorHost(activeHost.url)}</span>}
      </button>

      {open && (
        <div id="mirrors-panel" className="mt-2 flex min-h-0 flex-col gap-2">
          <ul className="flex max-h-32 flex-col gap-0.5 overflow-y-auto">
            {mirrors.length === 0 && <li className="px-3 py-1.5 text-card text-text-2">Загрузка…</li>}
            {mirrors.map((mirror) => (
              <MirrorRow
                key={mirror.url}
                mirror={mirror}
                disabled={!idle}
                onSelect={() => onSelect(mirror.url)}
              />
            ))}
          </ul>

          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={onCheck}
              disabled={!idle}
              className="flex h-8 items-center gap-2 rounded-button bg-white/10 px-3 text-card outline-none hover:bg-white/15 focus-visible:ring-2 focus-visible:ring-accent/70 disabled:opacity-40"
            >
              {busy === "checking" && <Spinner />}
              {busy === "checking" ? "Проверяем…" : checked ? "Проверить снова" : "Проверить"}
            </button>
            <form onSubmit={(event) => void onSubmit(event)} className="flex min-w-0 flex-1 gap-2">
              <input
                type="text"
                inputMode="url"
                autoComplete="off"
                autoCapitalize="off"
                autoCorrect="off"
                spellCheck={false}
                placeholder="Свой адрес зеркала"
                aria-label="Адрес зеркала"
                value={address}
                onChange={(event) => setAddress(event.target.value)}
                className="h-8 min-w-0 flex-1 rounded-button bg-white/10 px-3 text-card text-text outline-none placeholder:text-text-2 focus:ring-2 focus:ring-accent/70"
              />
              <button
                type="submit"
                disabled={!idle || address.trim() === ""}
                className="h-8 rounded-button bg-white/10 px-3 text-card outline-none hover:bg-white/15 focus-visible:ring-2 focus-visible:ring-accent/70 disabled:opacity-40"
              >
                Добавить
              </button>
            </form>
          </div>

          <p role="alert" className="min-h-5 text-card text-accent">
            {error}
          </p>
        </div>
      )}
    </section>
  );
}

function MirrorRow({
  mirror,
  disabled,
  onSelect,
}: {
  mirror: MirrorDto;
  disabled: boolean;
  onSelect: () => void;
}) {
  const info = STATE_INFO[mirror.state];
  return (
    <li>
      <button
        type="button"
        onClick={onSelect}
        disabled={disabled || mirror.active}
        aria-current={mirror.active ? "true" : undefined}
        title={mirror.reason ?? undefined}
        className={`flex h-8 w-full items-center gap-3 rounded-button px-3 text-left text-card outline-none focus-visible:ring-2 focus-visible:ring-accent/70 ${
          mirror.active ? "bg-white/10" : "hover:bg-white/5 disabled:opacity-60"
        }`}
      >
        <span aria-hidden="true" className={`h-2.5 w-2.5 shrink-0 rounded-full ${info.dot}`} />
        <span className="min-w-0 flex-1 truncate">
          {mirrorHost(mirror.url)}
          {mirror.standby && <span className="ml-2 text-text-2">резервный</span>}
        </span>
        <span className="shrink-0 text-text-2">
          {info.label}
          {mirror.latencyMs !== null && ` · ${Math.round(mirror.latencyMs)} мс`}
        </span>
        <span aria-hidden="true" className="w-4 shrink-0 text-center text-accent">
          {mirror.active ? "✓" : ""}
        </span>
        {mirror.active && <span className="sr-only">Активное зеркало</span>}
      </button>
    </li>
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
