import { useEffect, useState } from "react";

// Возвращает `true`, пока есть недавняя активность мыши/клавиатуры; скрывает
// контролы через `delayMs` после её прекращения. Слушатели вешаются на `window`,
// т.к. окно оверлея покрывает весь экран и обычные React-события на нём не
// сработают, когда клики проходят насквозь на окно `player` под ним.
export function useAutoHide(delayMs: number): boolean {
  const [visible, setVisible] = useState(true);

  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | null = null;

    const show = () => {
      setVisible(true);
      if (timer !== null) {
        clearTimeout(timer);
      }
      timer = setTimeout(() => setVisible(false), delayMs);
    };

    show();
    window.addEventListener("mousemove", show);
    window.addEventListener("keydown", show);
    return () => {
      if (timer !== null) {
        clearTimeout(timer);
      }
      window.removeEventListener("mousemove", show);
      window.removeEventListener("keydown", show);
    };
  }, [delayMs]);

  return visible;
}
