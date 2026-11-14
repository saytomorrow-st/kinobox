import { tauriApi } from "./api";
import { createSessionStore } from "./session-store";

export const useSessionStore = createSessionStore(tauriApi);
