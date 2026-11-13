//! Деобфускация ссылок потока HDrezka.
//!
//! Обфусцированная строка начинается с `#h`, содержит разделители `//_//`
//! и мусорные токены поверх base64. После снятия мусора и декодирования
//! получается список вида `[360p]url,[720p]url`, который разбирается
//! в [`StreamQuality`].

use url::Url;

use kino_core::{KinoError, Result, StreamQuality};

/// Мусорные токены, которые HDrezka вставляет в обфусцированную строку.
/// Обновлять при поломке.
const JUNK_TOKENS: &[&str] = &[
    "$$!!@$$@^!@#$$@",
    "@@@@@!##!^^^",
    "####^!!##!@@",
    "^^^!@##!!##",
    "$$#!!@#!@##",
];

/// Деобфусцирует `raw` в список доступных качеств потока.
///
/// Если `raw` (после `trim`) не начинается с `#h`, строка считается уже
/// распакованной и разбирается напрямую.
pub fn deobfuscate(raw: &str) -> Result<Vec<StreamQuality>> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(KinoError::Deobfuscation("пустой вход".to_string()));
    }

    let plain = match trimmed.strip_prefix("#h") {
        Some(rest) => {
            let joined: String = rest.split("//_//").collect();
            let mut cleaned = joined;
            for token in JUNK_TOKENS {
                cleaned = cleaned.replace(token, "");
            }
            let bytes = decode_base64(cleaned.trim())?;
            String::from_utf8(bytes)
                .map_err(|err| KinoError::Deobfuscation(format!("результат не UTF-8: {err}")))?
        }
        None => trimmed.to_string(),
    };

    parse_qualities(&plain)
}

/// base64-decode стандартным алфавитом; отсутствие `=`-padding допускается.
fn decode_base64(s: &str) -> Result<Vec<u8>> {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD_PAD_INDIFFERENT
        .decode(s)
        .map_err(|err| KinoError::Deobfuscation(format!("битый base64: {err}")))
}

/// Разбирает `[360p]url,[720p]url` в список качеств, сохраняя порядок.
fn parse_qualities(plain: &str) -> Result<Vec<StreamQuality>> {
    let mut qualities = Vec::new();
    let mut rest = plain.trim();

    while !rest.is_empty() {
        let after_open = rest.strip_prefix('[').ok_or_else(|| {
            KinoError::Deobfuscation("ожидалась метка качества вида [метка]url".to_string())
        })?;
        let (label, after_close) = after_open.split_once(']').ok_or_else(|| {
            KinoError::Deobfuscation("нет закрывающей ']' у метки качества".to_string())
        })?;
        let label = label.trim();
        if label.is_empty() {
            return Err(KinoError::Deobfuscation(
                "пустая метка качества".to_string(),
            ));
        }

        let (url_part, next) = split_at_next_quality(after_close);
        let url = Url::parse(url_part.trim()).map_err(|err| {
            KinoError::Deobfuscation(format!("невалидный url «{}»: {err}", url_part.trim()))
        })?;

        qualities.push(StreamQuality {
            label: label.to_string(),
            url,
        });
        rest = next;
    }

    if qualities.is_empty() {
        return Err(KinoError::Deobfuscation(
            "не найдено ни одного качества".to_string(),
        ));
    }

    Ok(qualities)
}

/// Делит `s` на url и остаток: url — до первой запятой, за которой сразу `[`.
/// Если такой запятой нет, весь `s` — url, остаток пустой.
fn split_at_next_quality(s: &str) -> (&str, &str) {
    match s.find(",[") {
        Some(idx) => {
            let (url, tail) = s.split_at(idx);
            let next = tail.strip_prefix(',').unwrap_or(tail);
            (url, next)
        }
        None => (s, ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;

    fn encode(plain: &str) -> String {
        base64::engine::general_purpose::STANDARD.encode(plain)
    }

    fn encode_no_pad(plain: &str) -> String {
        base64::engine::general_purpose::STANDARD_NO_PAD.encode(plain)
    }

    /// Вставляет фрагменты `pieces` (токен или разделитель и смещение в байтах)
    /// в base64-строку `b64`. Смещения заданы для исходной строки, поэтому
    /// вставки применяются справа налево, чтобы не сдвигать индексы.
    fn insert_at(b64: &str, pieces: &[(&str, usize)]) -> String {
        let mut sorted = pieces.to_vec();
        sorted.sort_by_key(|&(_, idx)| std::cmp::Reverse(idx));
        let mut out = b64.to_string();
        for (piece, idx) in sorted {
            out.insert_str(idx, piece);
        }
        out
    }

    /// Кодирует «в обратную сторону»: base64 без лишнего мусора + префикс `#h`.
    fn obfuscate(plain: &str) -> String {
        format!("#h{}", encode(plain))
    }

    const URLS: [&str; 3] = [
        "https://cdn.example.com/360.mp4",
        "https://cdn.example.com/720.mp4",
        "https://cdn.example.com/1080.mp4",
    ];

    #[test]
    fn multiple_qualities_obfuscated() {
        let plain = format!("[360p]{},[720p]{},[1080p]{}", URLS[0], URLS[1], URLS[2]);
        let b64 = encode(&plain);
        let raw = format!(
            "#h{}",
            insert_at(&b64, &[("$$!!@$$@^!@#$$@", 4), ("//_//", 21)])
        );

        let qualities = deobfuscate(&raw).expect("валидная строка");

        assert_eq!(qualities.len(), 3);
        assert_eq!(qualities[0].label, "360p");
        assert_eq!(qualities[0].url.as_str(), URLS[0]);
        assert_eq!(qualities[1].label, "720p");
        assert_eq!(qualities[1].url.as_str(), URLS[1]);
        assert_eq!(qualities[2].label, "1080p");
        assert_eq!(qualities[2].url.as_str(), URLS[2]);
    }

    #[test]
    fn obfuscated_with_junk_and_separators_in_many_places() {
        let plain = format!("[360p]{},[720p]{}", URLS[0], URLS[1]);
        let b64 = encode(&plain);
        let raw = format!(
            "#h{}",
            insert_at(
                &b64,
                &[
                    ("$$!!@$$@^!@#$$@", 3),
                    ("//_//", 10),
                    ("@@@@@!##!^^^", 25),
                    ("//_//", 33),
                    ("####^!!##!@@", 41),
                    ("//_//", 50),
                    ("^^^!@##!!##", 55),
                ]
            )
        );

        let qualities = deobfuscate(&raw).expect("валидная строка");

        assert_eq!(qualities.len(), 2);
        assert_eq!(qualities[0].label, "360p");
        assert_eq!(qualities[0].url.as_str(), URLS[0]);
        assert_eq!(qualities[1].label, "720p");
        assert_eq!(qualities[1].url.as_str(), URLS[1]);
    }

    #[test]
    fn plain_input_parsed_directly() {
        let raw = format!("[360p]{},[720p]{}", URLS[0], URLS[1]);

        let qualities = deobfuscate(&raw).expect("валидная строка");

        assert_eq!(qualities.len(), 2);
        assert_eq!(qualities[0].label, "360p");
        assert_eq!(qualities[0].url.as_str(), URLS[0]);
        assert_eq!(qualities[1].label, "720p");
        assert_eq!(qualities[1].url.as_str(), URLS[1]);
    }

    #[test]
    fn single_quality() {
        let raw = format!("[720p]{}", URLS[1]);

        let qualities = deobfuscate(&raw).expect("валидная строка");

        assert_eq!(qualities.len(), 1);
        assert_eq!(qualities[0].label, "720p");
        assert_eq!(qualities[0].url.as_str(), URLS[1]);
    }

    #[test]
    fn empty_and_whitespace_input() {
        assert!(matches!(deobfuscate(""), Err(KinoError::Deobfuscation(_))));
        assert!(matches!(
            deobfuscate("   \t\n"),
            Err(KinoError::Deobfuscation(_))
        ));
    }

    #[test]
    fn hash_prefix_with_broken_base64() {
        // `!` не входит в base64-алфавит и не является мусорным токеном целиком.
        assert!(matches!(
            deobfuscate("#h!!!!"),
            Err(KinoError::Deobfuscation(_))
        ));
    }

    #[test]
    fn valid_base64_but_not_utf8() {
        // 0xFF 0xFE — невалидный UTF-8.
        let raw = format!(
            "#h{}",
            base64::engine::general_purpose::STANDARD.encode([0xFFu8, 0xFE])
        );
        assert!(matches!(
            deobfuscate(&raw),
            Err(KinoError::Deobfuscation(_))
        ));
    }

    #[test]
    fn decoded_text_without_label() {
        let raw = obfuscate("no qualities here");
        assert!(matches!(
            deobfuscate(&raw),
            Err(KinoError::Deobfuscation(_))
        ));
    }

    #[test]
    fn invalid_url() {
        let raw = obfuscate("[720p]not a url");
        assert!(matches!(
            deobfuscate(&raw),
            Err(KinoError::Deobfuscation(_))
        ));
    }

    #[test]
    fn base64_without_padding_works() {
        let plain = format!("[720p]{}", URLS[1]);
        let raw = format!("#h{}", encode_no_pad(&plain));

        let qualities = deobfuscate(&raw).expect("base64 без padding");

        assert_eq!(qualities.len(), 1);
        assert_eq!(qualities[0].label, "720p");
        assert_eq!(qualities[0].url.as_str(), URLS[1]);
    }

    #[test]
    fn each_junk_token_is_removed() {
        let plain = format!("[720p]{}", URLS[1]);
        let b64 = encode(&plain);
        for token in JUNK_TOKENS {
            let raw = format!("#h{}", insert_at(&b64, &[(token, 7)]));
            let qualities = deobfuscate(&raw).expect("токен должен быть удалён");
            assert_eq!(qualities.len(), 1, "токен: {token}");
            assert_eq!(qualities[0].label, "720p");
            assert_eq!(qualities[0].url.as_str(), URLS[1]);
        }
    }

    #[test]
    fn url_with_comma_in_query_is_not_split() {
        // Запятая в query не заканчивает url, если за ней нет `[`.
        let raw = format!(
            "[720p]https://cdn.example.com/720.mp4?ref=a,b&x=1,[1080p]{}",
            URLS[2]
        );

        let qualities = deobfuscate(&raw).expect("валидная строка");

        assert_eq!(qualities.len(), 2);
        assert_eq!(qualities[0].label, "720p");
        assert_eq!(
            qualities[0].url.as_str(),
            "https://cdn.example.com/720.mp4?ref=a,b&x=1"
        );
        assert_eq!(qualities[1].label, "1080p");
        assert_eq!(qualities[1].url.as_str(), URLS[2]);
    }
}
