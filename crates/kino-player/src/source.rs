//! Загрузка `StreamQuality`/`SubtitleTrack` из `kino-core` в mpv и переключение качества
//! с сохранением позиции воспроизведения.

use kino_core::{StreamQuality, SubtitleTrack};

use crate::mpv::Player;

/// Для лога: `scheme://host:port` CDN-адреса без пути/query — там подписанные токены.
fn stream_origin(url: &url::Url) -> String {
    let mut origin = url.clone();
    origin.set_path("");
    origin.set_query(None);
    origin.set_fragment(None);
    origin.to_string()
}

impl Player {
    /// Загружает качество потока как текущий файл (`loadfile ... replace`).
    ///
    /// Используется прямой `url` качества. HLS-манифест и резервные mp4 (`fallbacks`)
    /// в этой задаче не задействованы — mpv сам умеет доигрывать HLS по прямому адресу,
    /// а выбор резервного адреса при обрыве основного решается в #018/#019.
    pub fn load_quality(&self, quality: &StreamQuality) -> libmpv2::Result<()> {
        // Полный CDN-адрес содержит подписанные токены в query — в лог только origin.
        log::info!("mpv loadfile: {}", stream_origin(&quality.url));
        self.mpv()
            .command("loadfile", &[quality.url.as_str(), "replace"])
    }

    /// Переключает качество, сохраняя текущую позицию воспроизведения: запоминает `position()`
    /// до загрузки нового файла и передаёт её как per-file опцию `start=` самой команде
    /// `loadfile` (а не отдельным `seek_to` после) — `loadfile` асинхронна (возвращается до того,
    /// как новый файл фактически загрузится), поэтому немедленный `seek` после неё почти всегда
    /// получает `MPV_ERROR_COMMAND` (нет текущего файла, которому можно сказать "перематывай"),
    /// а `start=` применяется самим mpv, когда файл будет готов.
    /// Для `None` (например пока позиция ещё не известна) опция не передаётся.
    pub fn switch_quality(&self, quality: &StreamQuality) -> libmpv2::Result<()> {
        let position = self.position()?;
        log::info!(
            "mpv switch_quality: {} (позиция {:?})",
            stream_origin(&quality.url),
            position
        );
        match position {
            Some(position) => self.mpv().command(
                "loadfile",
                &[
                    quality.url.as_str(),
                    "replace",
                    "-1",
                    &format!("start={position}"),
                ],
            ),
            None => self.load_quality(quality),
        }
    }

    /// Добавляет внешнюю дорожку субтитров (`sub-add`) к уже загруженному файлу.
    pub fn add_subtitle(&self, subtitle: &SubtitleTrack) -> libmpv2::Result<()> {
        let flag = if subtitle.is_default {
            "select"
        } else {
            "auto"
        };
        self.mpv().command(
            "sub-add",
            &[subtitle.url.as_str(), flag, &subtitle.label, &subtitle.lang],
        )
    }
}
