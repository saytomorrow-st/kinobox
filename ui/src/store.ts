import { tauriApi } from "./api";
import { createMirrorsStore } from "./mirrors-store";
import { createSessionStore } from "./session-store";

export const useSessionStore = createSessionStore(tauriApi);
export const useMirrorsStore = createMirrorsStore(tauriApi);
