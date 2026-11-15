import { CatalogScreen } from "../components/CatalogScreen";

export function MainScreen() {
  return (
    <main className="relative flex h-full flex-col">
      <div data-tauri-drag-region className="absolute inset-x-0 top-0 z-10 h-10" />
      <CatalogScreen />
    </main>
  );
}
