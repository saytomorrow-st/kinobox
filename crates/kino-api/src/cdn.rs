//! Чистые парсеры данных плеера и списка серий, без сети.
//!
//! Данные плеера на странице фильма лежат в вызове `sof.tv.initCDNMoviesEvents(...)`
//! (или `initCDNSeriesEvents(...)` для сериалов): числовые аргументы `post_id` и
//! `translator_id`, а последним аргументом — JSON-объект с полями `streams`
//! (обфусцированная строка потоков) и `thumbnails` (превью, пока не используется).
//!
//! Честные ограничения: `extract_player_data` проверен на реальной обезличенной
//! странице фильма `tests/fixtures/film_85038.html` (kvk.pub, 2026-11-14).
//! Разметка вызова `initCDNSeriesEvents` на реальной странице сериала не
//! проверялась — только синтетический фрагмент в тестах.

use scraper::{Html, Selector};

use kino_core::{Episode, KinoError, Result};

/// Маркеры вызова инициализации плеера на странице произведения.
const PLAYER_MARKERS: [&str; 2] = ["initCDNMoviesEvents(", "initCDNSeriesEvents("];

/// Данные плеера, извлечённые со страницы произведения.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerData {
    pub post_id: u32,
    pub translator_id: u32,
    /// Сырая (возможно обфусцированная) строка потоков из поля `"streams"`.
    pub streams: String,
}

/// Ищет в HTML первый вызов `initCDNMoviesEvents(...)` или
/// `initCDNSeriesEvents(...)` и извлекает из него `post_id`, `translator_id` и
/// строку `"streams"` последнего JSON-объекта.
///
/// Если вызова нет, JSON-объект битый или в нём нет строкового поля `streams` —
/// `KinoError::Parse`.
pub fn extract_player_data(html: &str) -> Result<PlayerData> {
    let (marker, call_start) = PLAYER_MARKERS
        .iter()
        .find_map(|marker| html.find(marker).map(|pos| (*marker, pos)))
        .ok_or_else(|| KinoError::Parse("на странице нет вызова initCDN...Events".to_string()))?;

    let args = &html[call_start + marker.len()..];
    let mut numbers = args
        .split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<u32>().ok());

    let post_id = numbers
        .next()
        .ok_or_else(|| KinoError::Parse("в вызове initCDN...Events нет post_id".to_string()))?;
    let translator_id = numbers.next().ok_or_else(|| {
        KinoError::Parse("в вызове initCDN...Events нет translator_id".to_string())
    })?;

    let json = extract_json_object(args)?;
    let value: serde_json::Value = serde_json::from_str(json)
        .map_err(|err| KinoError::Parse(format!("битый JSON плеера: {err}")))?;
    let streams = value
        .get("streams")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| KinoError::Parse("в JSON плеера нет поля streams".to_string()))?
        .to_string();

    Ok(PlayerData {
        post_id,
        translator_id,
        streams,
    })
}

/// Разбирает разметку списка серий `<ul><li>1 серия</li><li>2 серия</li>…</ul>`
/// в вектор [`Episode`]. `episode_id` — порядковый номер `<li>` начиная с 1,
/// `title` — текст элемента. Если `<ul>` нет или он пуст — `Ok(vec![])`.
pub fn parse_episodes_html(html: &str, season_id: u32) -> Result<Vec<Episode>> {
    let fragment = Html::parse_fragment(html);
    let li = Selector::parse("li")
        .map_err(|err| KinoError::Parse(format!("некорректный селектор li: {err}")))?;

    let mut episodes = Vec::new();
    for (index, element) in fragment.select(&li).enumerate() {
        let episode_id = u32::try_from(index + 1)
            .map_err(|_| KinoError::Parse("слишком много серий в сезоне".to_string()))?;
        let title = element.text().collect::<String>();
        episodes.push(Episode {
            season_id,
            episode_id,
            title: title.trim().to_string(),
        });
    }
    Ok(episodes)
}

/// Извлекает первый JSON-объект (по сбалансированным фигурным скобкам с учётом
/// строковых литералов) из текста аргументов вызова.
fn extract_json_object(args: &str) -> Result<&str> {
    let open = args.find('{').ok_or_else(|| {
        KinoError::Parse("в вызове initCDN...Events нет JSON-объекта".to_string())
    })?;

    let bytes = args.as_bytes();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (i, &byte) in bytes.iter().enumerate().skip(open) {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
        } else {
            match byte {
                b'"' => in_string = true,
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(&args[open..=i]);
                    }
                }
                _ => {}
            }
        }
    }
    Err(KinoError::Parse("JSON-объект плеера не закрыт".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn real_fixture() -> &'static str {
        include_str!("../tests/fixtures/film_85038.html")
    }

    #[test]
    fn extracts_player_data_from_real_fixture() {
        let player = extract_player_data(real_fixture()).expect("плеер из фикстуры");
        assert_eq!(player.post_id, 85038);
        assert_eq!(player.translator_id, 111);
        assert!(player.streams.starts_with("[360p]"), "{}", player.streams);
        assert!(player.streams.contains("1080p Ultra"));
    }

    #[test]
    fn extracts_player_data_from_series_call() {
        let html = r#"<html><body>
            <script>sof.tv.initCDNSeriesEvents(321, 45, 0, 0, 0, 'kvk.pub', true, false,
                {"id":"cdnplayer","streams":"","thumbnails":"/ajax/get_cdn_tiles/0/0/"}
            );</script>
        </body></html>"#;
        let player = extract_player_data(html).expect("плеер сериала");
        assert_eq!(player.post_id, 321);
        assert_eq!(player.translator_id, 45);
        assert_eq!(player.streams, "");
    }

    #[test]
    fn series_marker_with_streams_is_parsed() {
        // Первый же вызов разбирается; порядок маркеров не важен.
        let html = r#"<script>initCDNSeriesEvents(1, 2, 0, 0, 0, 'x', true, false,
            {"id":"cdnplayer","streams":"[360p]https://cdn.example.com/a.mp4"});</script>"#;
        let player = extract_player_data(html).expect("плеер");
        assert_eq!(player.post_id, 1);
        assert_eq!(player.translator_id, 2);
    }

    #[test]
    fn missing_call_is_parse_error() {
        let html = "<html><body><p>нет плеера</p></body></html>";
        assert!(matches!(
            extract_player_data(html),
            Err(KinoError::Parse(_))
        ));
    }

    #[test]
    fn broken_json_is_parse_error() {
        let html = r#"<script>initCDNMoviesEvents(85038, 111, 0, 0, 0, 'kvk.pub', true, false,
            {"id":"cdnplayer","streams": 123);</script>"#;
        assert!(matches!(
            extract_player_data(html),
            Err(KinoError::Parse(_))
        ));
    }

    #[test]
    fn missing_streams_field_is_parse_error() {
        let html = r#"<script>initCDNMoviesEvents(1, 2, 0, 0, 0, 'x', true, false,
            {"id":"cdnplayer"});</script>"#;
        assert!(matches!(
            extract_player_data(html),
            Err(KinoError::Parse(_))
        ));
    }

    #[test]
    fn parses_episodes_with_titles() {
        let html = "<ul><li>1 серия</li><li>2 серия</li><li>3 серия</li></ul>";
        let episodes = parse_episodes_html(html, 2).expect("серии");
        assert_eq!(episodes.len(), 3);
        for (index, episode) in episodes.iter().enumerate() {
            assert_eq!(episode.season_id, 2);
            assert_eq!(episode.episode_id, index as u32 + 1);
            assert_eq!(episode.title, format!("{} серия", index + 1));
        }
    }

    #[test]
    fn empty_ul_is_empty_list() {
        assert!(parse_episodes_html("<ul></ul>", 1)
            .expect("пустой ul")
            .is_empty());
        assert!(parse_episodes_html("<ul>  </ul>", 1)
            .expect("ul с пробелами")
            .is_empty());
    }

    #[test]
    fn missing_ul_is_empty_list() {
        assert!(parse_episodes_html("<div>нет списка</div>", 1)
            .expect("нет ul")
            .is_empty());
        assert!(parse_episodes_html("", 1)
            .expect("пустая строка")
            .is_empty());
    }

    #[test]
    fn li_with_and_without_text() {
        let html = "<ul><li>1 серия</li><li></li><li>3 серия</li></ul>";
        let episodes = parse_episodes_html(html, 5).expect("серии");
        assert_eq!(episodes.len(), 3);
        assert_eq!(episodes[0].title, "1 серия");
        assert_eq!(episodes[1].title, "");
        assert_eq!(episodes[2].title, "3 серия");
        assert_eq!(episodes[1].episode_id, 2);
    }
}
