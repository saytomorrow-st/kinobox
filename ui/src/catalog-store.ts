import { create } from "zustand";

import type { Api, MediaItemDto, MediaKindDto } from "./api";

export interface CatalogState {
  items: MediaItemDto[];
  kind: MediaKindDto | null;
  page: number;
  loading: boolean;
  loadingMore: boolean;
  hasMore: boolean;
  error: string | null;
  query: string;
  load: (kind?: MediaKindDto | null) => Promise<void>;
  loadMore: () => Promise<void>;
  search: (query: string) => Promise<void>;
}

const ERROR_MESSAGE = "Не удалось загрузить каталог. Попробуйте ещё раз.";

export function createCatalogStore(api: Api) {
  let inflight: Promise<void> | null = null;

  return create<CatalogState>()((set, get) => ({
    items: [],
    kind: null,
    page: 1,
    loading: false,
    loadingMore: false,
    hasMore: true,
    error: null,
    query: "",

    load: async (kind) => {
      if (get().loading) return;
      const nextKind = kind === undefined ? get().kind : kind;
      set({ loading: true, error: null, kind: nextKind, query: "" });
      const outcome = await api.catalog(nextKind, 1);
      if (outcome.ok) {
        set({
          items: outcome.value,
          page: 1,
          hasMore: outcome.value.length > 0,
          loading: false,
        });
      } else {
        set({ loading: false, error: ERROR_MESSAGE });
      }
    },

    loadMore: async () => {
      const state = get();
      if (state.loading || state.loadingMore || !state.hasMore || state.query) return;
      if (inflight) return inflight;
      const nextPage = state.page + 1;
      set({ loadingMore: true, error: null });
      inflight = (async () => {
        const outcome = await api.catalog(state.kind, nextPage);
        if (outcome.ok) {
          set((prev) => ({
            items: [...prev.items, ...outcome.value],
            page: nextPage,
            hasMore: outcome.value.length > 0,
            loadingMore: false,
          }));
        } else {
          set({ loadingMore: false, error: ERROR_MESSAGE });
        }
      })();
      try {
        await inflight;
      } finally {
        inflight = null;
      }
    },

    search: async (query) => {
      if (get().loading) return;
      const trimmed = query.trim();
      if (trimmed === "") {
        await get().load(get().kind);
        return;
      }
      set({ loading: true, error: null, query: trimmed });
      const outcome = await api.search(trimmed);
      if (outcome.ok) {
        set({ items: outcome.value, page: 1, hasMore: false, loading: false });
      } else {
        set({ loading: false, error: ERROR_MESSAGE });
      }
    },
  }));
}
