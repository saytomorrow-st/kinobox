// Чистая функция форматирования времени для плеера. Вынесена отдельно, чтобы
// тесты могли импортировать её напрямую, не трогая DOM-компонент.
export function formatTime(seconds: number | null): string {
  if (seconds === null) {
    return "--:--";
  }
  // Отрицательные позиции (результат перемотки за начало) зажимаем в ноль.
  const total = Math.max(0, Math.floor(seconds));
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const secs = total % 60;
  const mm = String(minutes).padStart(2, "0");
  const ss = String(secs).padStart(2, "0");
  if (hours > 0) {
    return `${hours}:${mm}:${ss}`;
  }
  return `${mm}:${ss}`;
}
