import { useEffect, useState, type FormEvent, type ReactNode } from "react";

import appIcon from "../assets/app-icon.png";
import { MirrorsPanel } from "../components/MirrorsPanel";
import { useSessionStore } from "../store";

const LOGIN_FORM_ID = "login-form";

export function LoginScreen() {
  const phase = useSessionStore((state) => state.phase);
  const email = useSessionStore((state) => state.email);
  const message = useSessionStore((state) => state.message);
  const boot = useSessionStore((state) => state.boot);

  useEffect(() => {
    void boot();
  }, [boot]);

  return (
    <main className="relative flex h-full flex-col px-8 pb-6 pt-9">
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

// Иконка слева, текст и всё остальное справа, как в диалоге «Требуется вход» Apple TV.
// Блок зеркал лежит под текстом, в той же колонке, и прокручивается вместе с ней, если не помещается.
function DialogLayout({
  form,
  children,
  footer,
}: {
  form: ReactNode;
  children?: ReactNode;
  footer: ReactNode;
}) {
  return (
    <>
      <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto">
        <div className="flex gap-5">
          <img
            src={appIcon}
            alt=""
            draggable={false}
            className="h-[72px] w-[72px] shrink-0 select-none"
          />
          <div className="flex min-w-0 flex-1 flex-col">{form}</div>
        </div>
        <div className="pl-[92px]">{children}</div>
      </div>
      <div className="mt-4 flex justify-end gap-3">{footer}</div>
    </>
  );
}

function LoginForm({ initialEmail }: { initialEmail: string }) {
  const [email, setEmail] = useState(initialEmail);
  const [password, setPassword] = useState("");
  const [mirrorsOpen, setMirrorsOpen] = useState(false);
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
    <DialogLayout
      form={
        <form id={LOGIN_FORM_ID} onSubmit={onSubmit} className="flex flex-col">
          <h1 className="text-section">Требуется вход</h1>
          <p className="mt-1 text-card text-text-2">Войдите в аккаунт HDrezka, чтобы продолжить.</p>

          <div className="mt-4 flex flex-col gap-3">
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
        </form>
      }
      footer={
        <>
          <button
            type="button"
            onClick={() => void quit()}
            className="h-9 rounded-button bg-white/10 px-5 text-card outline-none hover:bg-white/15 focus-visible:ring-2 focus-visible:ring-accent/70"
          >
            Отменить
          </button>
          <button
            type="submit"
            form={LOGIN_FORM_ID}
            disabled={!canSubmit}
            className="h-9 rounded-button bg-accent px-5 text-card font-semibold text-white outline-none focus-visible:ring-2 focus-visible:ring-white/70 disabled:opacity-40"
          >
            Войти
          </button>
        </>
      }
    >
      <MirrorsPanel open={mirrorsOpen} onToggle={() => setMirrorsOpen((open) => !open)} />
    </DialogLayout>
  );
}

function Unavailable({ message, onRetry }: { message: string; onRetry: () => void }) {
  const [mirrorsOpen, setMirrorsOpen] = useState(true);

  return (
    <DialogLayout
      form={
        <>
          <h1 className="text-section">Нет соединения</h1>
          <p role="alert" className="mt-1 text-card text-text-2">
            {message}
          </p>
        </>
      }
      footer={
        <button
          type="button"
          onClick={onRetry}
          className="h-9 rounded-button bg-accent px-5 text-card font-semibold text-white outline-none focus-visible:ring-2 focus-visible:ring-white/70"
        >
          Повторить
        </button>
      }
    >
      <MirrorsPanel open={mirrorsOpen} onToggle={() => setMirrorsOpen((open) => !open)} />
    </DialogLayout>
  );
}
