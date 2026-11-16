import { useEffect } from "react";

import { usePlayerStore } from "../store";
import { PlayerControlsView } from "./PlayerControlsView";

export function PlayerControlsPanel() {
  const qualities = usePlayerStore((state) => state.qualities);
  const qualitiesLoading = usePlayerStore((state) => state.qualitiesLoading);
  const currentQuality = usePlayerStore((state) => state.currentQuality);
  const position = usePlayerStore((state) => state.position);
  const duration = usePlayerStore((state) => state.duration);
  const paused = usePlayerStore((state) => state.paused);
  const volume = usePlayerStore((state) => state.volume);
  const error = usePlayerStore((state) => state.error);
  const init = usePlayerStore((state) => state.init);
  const dispose = usePlayerStore((state) => state.dispose);
  const play = usePlayerStore((state) => state.play);
  const pause = usePlayerStore((state) => state.pause);
  const seek = usePlayerStore((state) => state.seek);
  const setQuality = usePlayerStore((state) => state.setQuality);
  const setVolume = usePlayerStore((state) => state.setVolume);
  const close = usePlayerStore((state) => state.close);

  // Окно `player-controls` живёт отдельно от `main`: своя подписка на `player-progress`
  // заводится при монтировании и снимается при закрытии — `init`/`dispose` идемпотентны.
  useEffect(() => {
    void init();
    return () => dispose();
  }, [init, dispose]);

  return (
    <PlayerControlsView
      qualities={qualities}
      qualitiesLoading={qualitiesLoading}
      currentQuality={currentQuality}
      position={position}
      duration={duration}
      paused={paused}
      volume={volume}
      error={error}
      onPlay={() => void play()}
      onPause={() => void pause()}
      onSeek={(seconds) => void seek(seconds)}
      onSelectQuality={(quality) => void setQuality(quality)}
      onSetVolume={(value) => void setVolume(value)}
      onClose={() => void close()}
    />
  );
}
