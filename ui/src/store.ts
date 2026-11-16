import { tauriApi } from "./api";
import { createCatalogStore } from "./catalog-store";
import { createDetailsStore } from "./details-store";
import { createMirrorsStore } from "./mirrors-store";
import { createSessionStore } from "./session-store";

export const useSessionStore = createSessionStore(tauriApi);
export const useMirrorsStore = createMirrorsStore(tauriApi);
export const useCatalogStore = createCatalogStore(tauriApi);
export const useDetailsStore = createDetailsStore(tauriApi);
