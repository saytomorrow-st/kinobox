import { useEffect } from "react";

import { useSessionStore } from "../store";

export function MainScreen() {
  const user = useSessionStore((state) => state.user);
  const error = useSessionStore((state) => state.error);
  const loadSession = useSessionStore((state) => state.loadSession);
  const logout = useSessionStore((state) => state.logout);

  useEffect(() => {
    void loadSession();
  }, [loadSession]);

  return (
    <main className="flex h-full flex-col justify-center px-page">
      <div data-tauri-drag-region className="absolute inset-x-0 top-0 h-10" />
      {user && (
        <>
          <h1 className="text-hero">Вы вошли как {user.username}</h1>
          <div className="mt-section">
            <button
              type="button"
              onClick={() => void logout()}
              className="h-10 rounded-button bg-white/10 px-6 text-card hover:bg-white/15"
            >
              Выйти
            </button>
          </div>
          {error && (
            <p role="alert" className="mt-4 text-card text-accent">
              {error}
            </p>
          )}
        </>
      )}
    </main>
  );
}
