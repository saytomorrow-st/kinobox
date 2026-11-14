import { useEffect } from "react";

import { useMirrorsStore } from "../store";
import { MirrorsView } from "./MirrorsView";

interface MirrorsPanelProps {
  open: boolean;
  onToggle: () => void;
}

export function MirrorsPanel({ open, onToggle }: MirrorsPanelProps) {
  const mirrors = useMirrorsStore((state) => state.mirrors);
  const checked = useMirrorsStore((state) => state.checked);
  const busy = useMirrorsStore((state) => state.busy);
  const error = useMirrorsStore((state) => state.error);
  const load = useMirrorsStore((state) => state.load);
  const check = useMirrorsStore((state) => state.check);
  const add = useMirrorsStore((state) => state.add);
  const select = useMirrorsStore((state) => state.select);

  // `load` не ходит в сеть. Проверка (`check`) запускается только кнопкой, а не при каждом показе.
  useEffect(() => {
    void load();
  }, [load]);

  return (
    <MirrorsView
      open={open}
      onToggle={onToggle}
      mirrors={mirrors}
      checked={checked}
      busy={busy}
      error={error}
      onCheck={() => void check()}
      onSelect={(url) => void select(url)}
      onAdd={add}
    />
  );
}
