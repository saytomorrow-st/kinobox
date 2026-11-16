import { useDetailsStore } from "../store";
import { DetailsView } from "./DetailsView";

export function DetailsModal() {
  const open = useDetailsStore((state) => state.open);
  const item = useDetailsStore((state) => state.item);
  const details = useDetailsStore((state) => state.details);
  const loading = useDetailsStore((state) => state.loading);
  const error = useDetailsStore((state) => state.error);
  const translatorId = useDetailsStore((state) => state.translatorId);
  const episodes = useDetailsStore((state) => state.episodes);
  const episodesLoading = useDetailsStore((state) => state.episodesLoading);
  const episodeIndex = useDetailsStore((state) => state.episodeIndex);
  const launching = useDetailsStore((state) => state.launching);
  const launchError = useDetailsStore((state) => state.launchError);
  const close = useDetailsStore((state) => state.close);
  const selectTranslator = useDetailsStore((state) => state.selectTranslator);
  const selectEpisode = useDetailsStore((state) => state.selectEpisode);
  const play = useDetailsStore((state) => state.play);

  if (!open) {
    return null;
  }

  return (
    <DetailsView
      item={item}
      details={details}
      loading={loading}
      error={error}
      translatorId={translatorId}
      episodes={episodes}
      episodesLoading={episodesLoading}
      episodeIndex={episodeIndex}
      launching={launching}
      launchError={launchError}
      onClose={close}
      onSelectTranslator={selectTranslator}
      onSelectEpisode={selectEpisode}
      onPlay={() => void play()}
    />
  );
}
