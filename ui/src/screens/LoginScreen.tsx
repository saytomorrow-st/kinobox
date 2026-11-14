import { useEffect, useState, type FormEvent } from "react";

import { useSessionStore } from "../store";

export function LoginScreen() {
  const phase = useSessionStore((state) => state.phase);
  const email = useSessionStore((state) => state.email);
  const message = useSessionStore((state) => state.message);
  const boot = useSessionStore((state) => state.boot);

  useEffect(() => {
    void boot();
  }, [boot]);

  return (
    <main className="relative flex h-full flex-col p-8">
      <div data-tauri-drag-region className="absolute inset-x-0 top-0 h-10" />
      {phase === "needsLogin" ? (
        <LoginForm initialEmail={email ?? ""} />
      ) : phase === "unavailable" ? (
        <Unavailable message={message ?? ""} onRetry={() => void boot()} />
      ) : (
        <p className="m-auto text-card text-text-2">
          {phase === "signedIn" ? "Открываем KinoBox…" : "Подключение…"}
        </p>
      )}
    </main>
  );
}

function LoginForm({ initialEmail }: { initialEmail: string }) {
  const [email, setEmail] = useState(initialEmail);
  const [password, setPassword] = useState("");
  const error = useSessionStore((state) => state.error);
  const submitting = useSessionStore((state) => state.submitting);
  const login = useSessionStore((state) => state.login);
  const quit = useSessionStore((state) => state.quit);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        void quit();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [quit]);

  const canSubmit = email.trim() !== "" && password !== "" && !submitting;

  const onSubmit = (event: FormEvent) => {
    event.preventDefault();
    if (!canSubmit) {
      return;
    }
    const secret = password;
    setPassword("");
    void login(email, secret);
  };

  const field =
    "h-9 w-full rounded-button bg-white/10 px-3 text-card text-text placeholder:text-text-2 outline-none focus:ring-2 focus:ring-accent/70";

  return (
    <form onSubmit={onSubmit} className="flex h-full flex-col pt-4">
      <h1 className="text-section">Требуется вход</h1>
      <p className="mt-1 text-card text-text-2">Войдите в аккаунт HDrezka, чтобы продолжить.</p>

      <div className="mt-5 flex flex-col gap-3">
        <input
          type="text"
          inputMode="email"
          autoComplete="off"
          autoCapitalize="off"
          autoCorrect="off"
          spellCheck={false}
          autoFocus={initialEmail === ""}
          placeholder="Email"
          aria-label="Email"
          value={email}
          onChange={(event) => setEmail(event.target.value)}
          className={field}
        />
        <input
          type="password"
          autoComplete="off"
          autoFocus={initialEmail !== ""}
          placeholder="Пароль"
          aria-label="Пароль"
          value={password}
          onChange={(event) => setPassword(event.target.value)}
          className={field}
        />
      </div>

      <p role="alert" className="mt-3 min-h-5 text-card text-accent">
        {error}
      </p>

      <div className="mt-auto flex justify-end gap-3">
        <button
          type="button"
          onClick={() => void quit()}
          className="h-9 rounded-button bg-white/10 px-5 text-card hover:bg-white/15"
        >
          Отменить
        </button>
        <button
          type="submit"
          disabled={!canSubmit}
          className="h-9 rounded-button bg-accent px-5 text-card font-semibold text-white disabled:opacity-40"
        >
          Войти
        </button>
      </div>
    </form>
  );
}

function Unavailable({ message, onRetry }: { message: string; onRetry: () => void }) {
  return (
    <div className="flex h-full flex-col pt-4">
      <h1 className="text-section">Нет соединения</h1>
      <p className="mt-2 text-card text-text-2">{message}</p>
      <div className="mt-auto flex justify-end">
        <button
          type="button"
          onClick={onRetry}
          className="h-9 rounded-button bg-accent px-5 text-card font-semibold text-white"
        >
          Повторить
        </button>
      </div>
    </div>
  );
}
