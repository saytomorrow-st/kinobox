//! Деобфускация ссылок потока HDrezka.
//!
//! Обфусцированная строка начинается с `#h`, содержит разделители `//_//`
//! и мусорные токены поверх base64. После снятия мусора и декодирования
//! получается список качеств, который разбирается в [`StreamQuality`].
//!
//! Формат одного качества: `[метка]адрес or адрес …`, адреса внутри качества
//! разделены точной подстрокой ` or `. Метка может содержать HTML
//! (`[<span class="pjs-prem-quality">1080p Ultra<img …></span>]`). Качества
//! разделены запятой, за которой сразу начинается следующая метка `[`.
//! Адрес, оканчивающийся на `:hls:manifest.m3u8`, — HLS, остальные — прямые mp4.

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

/// Суффикс HLS-адресов внутри качества.
const HLS_SUFFIX: &str = ":hls:manifest.m3u8";

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

/// Разбирает список качеств в вектор, сохраняя порядок.
fn parse_qualities(plain: &str) -> Result<Vec<StreamQuality>> {
    let mut qualities = Vec::new();
    let mut rest = plain.trim();

    while !rest.is_empty() {
        let after_open = rest.strip_prefix('[').ok_or_else(|| {
            KinoError::Deobfuscation("ожидалась метка качества вида [метка]адрес".to_string())
        })?;
        let close = find_label_close(after_open)?;
        let raw_label = &after_open[..close];
        let after_close = &after_open[close + 1..];

        let (urls_part, next) = split_quality_urls(after_close);
        qualities.push(build_quality(raw_label, urls_part)?);
        rest = next;
    }

    if qualities.is_empty() {
        return Err(KinoError::Deobfuscation(
            "не найдено ни одного качества".to_string(),
        ));
    }

    Ok(qualities)
}

/// Находит байтовый индекс закрывающей `]` метки внутри `content` (текст сразу
/// после `[`). Если метка начинается с `<` (HTML), `]` ищется после закрывающего
/// `</span>`; иначе — первая `]`, стоящая вне HTML-тега. Простой конечный
/// автомат по символам, без регулярных выражений; границы всегда по UTF-8.
fn find_label_close(content: &str) -> Result<usize> {
    let html = content.starts_with('<');
    let mut in_tag = false;
    let mut tag_body_start = 0usize;
    let mut seen_closing_span = false;

    for (i, ch) in content.char_indices() {
        if in_tag {
            if ch == '>' {
                in_tag = false;
                if content[tag_body_start..i].trim() == "/span" {
                    seen_closing_span = true;
                }
            }
        } else if ch == '<' {
            in_tag = true;
            tag_body_start = i + 1;
        } else if ch == ']' && (!html || seen_closing_span) {
            return Ok(i);
        }
    }

    Err(KinoError::Deobfuscation(
        "нет закрывающей ']' у метки качества".to_string(),
    ))
}

/// Делит `s` (текст сразу после `]` метки) на адреса текущего качества и
/// остаток. Разделитель качеств — запятая, за которой сразу `[`; поэтому
/// запятая внутри url (например, в query) не ломает разбор.
fn split_quality_urls(s: &str) -> (&str, &str) {
    match s.find(",[") {
        Some(idx) => {
            let (urls, tail) = s.split_at(idx);
            let next = tail.strip_prefix(',').unwrap_or(tail);
            (urls, next)
        }
        None => (s, ""),
    }
}

/// Собирает [`StreamQuality`] из сырой метки и строки адресов.
fn build_quality(raw_label: &str, urls_part: &str) -> Result<StreamQuality> {
    let label = strip_html(raw_label)?;
    let premium = raw_label.contains("pjs-prem-quality");

    let urls_part = urls_part.trim();
    if urls_part.is_empty() {
        return Err(KinoError::Deobfuscation(
            "нет адресов у качества".to_string(),
        ));
    }

    let mut direct: Vec<&str> = Vec::new();
    let mut hls_first: Option<&str> = None;
    for addr in urls_part.split(" or ") {
        let addr = addr.trim();
        if addr.is_empty() {
            continue;
        }
        if addr.ends_with(HLS_SUFFIX) {
            if hls_first.is_none() {
                hls_first = Some(addr);
            }
            // Остальные HLS-адреса игнорируем, но не падаем.
        } else {
            direct.push(addr);
        }
    }

    // Основной прямой mp4. Если прямых нет, а HLS есть — выводим адрес HLS
    // без суффикса `:hls:manifest.m3u8` (допущение, так делает одна из
    // неофициальных библиотек; реальный прямой адрес сайт может не отдавать).
    let url_str = match direct.first() {
        Some(first) => *first,
        None => match hls_first {
            Some(hls_str) => hls_str.strip_suffix(HLS_SUFFIX).ok_or_else(|| {
                KinoError::Deobfuscation("не удалось снять суффикс HLS".to_string())
            })?,
            None => {
                return Err(KinoError::Deobfuscation(
                    "нет адресов у качества".to_string(),
                ))
            }
        },
    };

    let url = Url::parse(url_str)
        .map_err(|err| KinoError::Deobfuscation(format!("невалидный url «{url_str}»: {err}")))?;

    let hls =
        match hls_first {
            Some(s) => Some(Url::parse(s).map_err(|err| {
                KinoError::Deobfuscation(format!("невалидный HLS url «{s}»: {err}"))
            })?),
            None => None,
        };

    let fallbacks = direct
        .iter()
        .skip(1)
        .map(|s| {
            Url::parse(s)
                .map_err(|err| KinoError::Deobfuscation(format!("невалидный url «{s}»: {err}")))
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(StreamQuality {
        label,
        url,
        premium,
        hls,
        fallbacks,
    })
}

/// Вырезает HTML-теги из метки (всё между `<` и `>`) и убирает пробелы по краям.
/// Пустая метка после очистки — ошибка.
fn strip_html(label: &str) -> Result<String> {
    let mut out = String::new();
    let mut in_tag = false;
    for ch in label.chars() {
        if ch == '<' {
            in_tag = true;
        } else if ch == '>' {
            in_tag = false;
        } else if !in_tag {
            out.push(ch);
        }
    }
    let cleaned = out.trim().to_string();
    if cleaned.is_empty() {
        return Err(KinoError::Deobfuscation(
            "пустая метка качества".to_string(),
        ));
    }
    Ok(cleaned)
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

    /// Прямой mp4-адрес на хосте `host` с индексом `idx`.
    fn cdn_mp4(host: &str, idx: &str) -> String {
        format!("https://{host}/s/{idx}/zrkms.mp4")
    }

    /// HLS-адрес на хосте `host` с индексом `idx`.
    fn cdn_hls(host: &str, idx: &str) -> String {
        format!("https://{host}/s/{idx}/zrkms.mp4:hls:manifest.m3u8")
    }

    /// Четыре адреса одного качества: два HLS и два прямых на двух хостах.
    fn quality_urls(idx: &str) -> String {
        format!(
            "{} or {} or {} or {}",
            cdn_hls("a.example.com", idx),
            cdn_hls("b.example.com", idx),
            cdn_mp4("a.example.com", idx),
            cdn_mp4("b.example.com", idx),
        )
    }

    /// Обычное качество в реальном формате.
    fn quality(label: &str, idx: &str) -> String {
        format!("[{label}]{}", quality_urls(idx))
    }

    /// Премиум-качество с HTML-меткой.
    fn premium_quality(idx: &str) -> String {
        format!(
            "[<span class=\"pjs-prem-quality\">1080p Ultra<img src=\"https://cdn.example.com/prem-icon.svg\" alt=\"\"></span>]{}",
            quality_urls(idx)
        )
    }

    // --- реальный формат ---

    #[test]
    fn real_format_with_premium_html_label() {
        let plain = format!(
            "{},{},{},{},{}",
            quality("360p", "1"),
            quality("480p", "2"),
            quality("720p", "3"),
            quality("1080p", "4"),
            premium_quality("5"),
        );

        let qualities = deobfuscate(&plain).expect("валидная строка");

        assert_eq!(qualities.len(), 5);

        for (i, expected_label) in ["360p", "480p", "720p", "1080p"].iter().enumerate() {
            let q = &qualities[i];
            assert_eq!(&q.label, expected_label);
            assert!(!q.premium, "качество {expected_label} не премиум");
            assert_eq!(
                q.url.as_str(),
                cdn_mp4("a.example.com", &(i + 1).to_string())
            );
            assert_eq!(
                q.hls.as_ref().unwrap().as_str(),
                cdn_hls("a.example.com", &(i + 1).to_string())
            );
            assert_eq!(q.fallbacks.len(), 1);
            assert_eq!(
                q.fallbacks[0].as_str(),
                cdn_mp4("b.example.com", &(i + 1).to_string())
            );
        }

        let p = &qualities[4];
        assert_eq!(p.label, "1080p Ultra");
        assert!(p.premium);
        assert_eq!(p.url.as_str(), cdn_mp4("a.example.com", "5"));
        assert_eq!(
            p.hls.as_ref().unwrap().as_str(),
            cdn_hls("a.example.com", "5")
        );
        assert_eq!(p.fallbacks.len(), 1);
        assert_eq!(p.fallbacks[0].as_str(), cdn_mp4("b.example.com", "5"));
    }

    #[test]
    fn only_hls_addresses_derive_mp4_url() {
        let plain = format!(
            "[720p]{} or {}",
            cdn_hls("a.example.com", "7"),
            cdn_hls("b.example.com", "7"),
        );

        let qualities = deobfuscate(&plain).expect("валидная строка");

        assert_eq!(qualities.len(), 1);
        let q = &qualities[0];
        assert_eq!(q.label, "720p");
        assert!(!q.premium);
        // url выводится срезом суффикса `:hls:manifest.m3u8`.
        assert_eq!(q.url.as_str(), cdn_mp4("a.example.com", "7"));
        assert_eq!(
            q.hls.as_ref().unwrap().as_str(),
            cdn_hls("a.example.com", "7")
        );
        assert!(q.fallbacks.is_empty());
    }

    #[test]
    fn only_mp4_no_hls() {
        let plain = format!(
            "[480p]{} or {}",
            cdn_mp4("a.example.com", "8"),
            cdn_mp4("b.example.com", "8"),
        );

        let qualities = deobfuscate(&plain).expect("валидная строка");

        assert_eq!(qualities.len(), 1);
        let q = &qualities[0];
        assert_eq!(q.label, "480p");
        assert!(!q.premium);
        assert_eq!(q.url.as_str(), cdn_mp4("a.example.com", "8"));
        assert!(q.hls.is_none());
        assert_eq!(q.fallbacks.len(), 1);
        assert_eq!(q.fallbacks[0].as_str(), cdn_mp4("b.example.com", "8"));
    }

    #[test]
    fn html_label_with_special_chars_in_attribute() {
        // В метке `>` и `"` (синтаксис тега), в атрибуте img — `/` и `:`.
        let plain = format!(
            "[<span class=\"pjs-prem-quality\">4K Ultra<img src=\"https://cdn.example.com/icons/prem.svg\" alt=\"\"></span>]{}",
            quality_urls("9")
        );

        let qualities = deobfuscate(&plain).expect("валидная строка");

        assert_eq!(qualities.len(), 1);
        let q = &qualities[0];
        assert_eq!(q.label, "4K Ultra");
        assert!(q.premium);
        assert_eq!(q.url.as_str(), cdn_mp4("a.example.com", "9"));
        assert_eq!(
            q.hls.as_ref().unwrap().as_str(),
            cdn_hls("a.example.com", "9")
        );
        assert_eq!(q.fallbacks.len(), 1);
    }

    #[test]
    fn comma_in_query_with_or_separator_is_not_split() {
        let plain = format!(
            "[720p]https://a.example.com/s/10/zrkms.mp4?ref=a,b&x=1 or https://a.example.com/s/10/alt.mp4,[1080p]{}",
            cdn_mp4("a.example.com", "11")
        );

        let qualities = deobfuscate(&plain).expect("валидная строка");

        assert_eq!(qualities.len(), 2);
        assert_eq!(qualities[0].label, "720p");
        assert_eq!(
            qualities[0].url.as_str(),
            "https://a.example.com/s/10/zrkms.mp4?ref=a,b&x=1"
        );
        assert_eq!(qualities[0].fallbacks.len(), 1);
        assert_eq!(
            qualities[0].fallbacks[0].as_str(),
            "https://a.example.com/s/10/alt.mp4"
        );
        assert_eq!(qualities[1].label, "1080p");
        assert_eq!(qualities[1].url.as_str(), cdn_mp4("a.example.com", "11"));
    }

    #[test]
    fn cyrillic_label_does_not_break_boundaries() {
        let plain = format!(
            "[720p Дубляж]{},[1080p Оригинал]{}",
            cdn_mp4("a.example.com", "12"),
            cdn_mp4("a.example.com", "13"),
        );

        let qualities = deobfuscate(&plain).expect("валидная строка");

        assert_eq!(qualities.len(), 2);
        assert_eq!(qualities[0].label, "720p Дубляж");
        assert_eq!(qualities[0].url.as_str(), cdn_mp4("a.example.com", "12"));
        assert_eq!(qualities[1].label, "1080p Оригинал");
        assert_eq!(qualities[1].url.as_str(), cdn_mp4("a.example.com", "13"));
    }

    #[test]
    fn obfuscated_input_with_new_format() {
        let plain = format!(
            "{},{},{}",
            quality("360p", "1"),
            quality("720p", "3"),
            premium_quality("5")
        );
        let b64 = encode(&plain);
        let raw = format!(
            "#h{}",
            insert_at(&b64, &[("$$!!@$$@^!@#$$@", 4), ("//_//", 21)])
        );

        let qualities = deobfuscate(&raw).expect("валидная строка");

        assert_eq!(qualities.len(), 3);
        assert_eq!(qualities[0].label, "360p");
        assert_eq!(qualities[0].url.as_str(), cdn_mp4("a.example.com", "1"));
        assert_eq!(qualities[2].label, "1080p Ultra");
        assert!(qualities[2].premium);
        assert_eq!(qualities[2].url.as_str(), cdn_mp4("a.example.com", "5"));
    }

    // --- старые сценарии (смысл сохранён) ---

    #[test]
    fn multiple_qualities_obfuscated() {
        let plain = format!(
            "[360p]{},[720p]{},[1080p]{}",
            cdn_mp4("cdn.example.com", "360"),
            cdn_mp4("cdn.example.com", "720"),
            cdn_mp4("cdn.example.com", "1080"),
        );
        let b64 = encode(&plain);
        let raw = format!(
            "#h{}",
            insert_at(&b64, &[("$$!!@$$@^!@#$$@", 4), ("//_//", 21)])
        );

        let qualities = deobfuscate(&raw).expect("валидная строка");

        assert_eq!(qualities.len(), 3);
        assert_eq!(qualities[0].label, "360p");
        assert_eq!(qualities[0].url.as_str(), cdn_mp4("cdn.example.com", "360"));
        assert!(qualities[0].hls.is_none());
        assert!(qualities[0].fallbacks.is_empty());
        assert_eq!(qualities[1].label, "720p");
        assert_eq!(qualities[1].url.as_str(), cdn_mp4("cdn.example.com", "720"));
        assert_eq!(qualities[2].label, "1080p");
        assert_eq!(
            qualities[2].url.as_str(),
            cdn_mp4("cdn.example.com", "1080")
        );
    }

    #[test]
    fn obfuscated_with_junk_and_separators_in_many_places() {
        let plain = format!(
            "[360p]{},[720p]{}",
            cdn_mp4("cdn.example.com", "360"),
            cdn_mp4("cdn.example.com", "720"),
        );
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
        assert_eq!(qualities[0].url.as_str(), cdn_mp4("cdn.example.com", "360"));
        assert_eq!(qualities[1].label, "720p");
        assert_eq!(qualities[1].url.as_str(), cdn_mp4("cdn.example.com", "720"));
    }

    #[test]
    fn plain_input_parsed_directly() {
        let raw = format!(
            "[360p]{},[720p]{}",
            cdn_mp4("cdn.example.com", "360"),
            cdn_mp4("cdn.example.com", "720"),
        );

        let qualities = deobfuscate(&raw).expect("валидная строка");

        assert_eq!(qualities.len(), 2);
        assert_eq!(qualities[0].label, "360p");
        assert_eq!(qualities[0].url.as_str(), cdn_mp4("cdn.example.com", "360"));
        assert_eq!(qualities[1].label, "720p");
        assert_eq!(qualities[1].url.as_str(), cdn_mp4("cdn.example.com", "720"));
    }

    #[test]
    fn single_quality() {
        let raw = format!("[720p]{}", cdn_mp4("cdn.example.com", "720"));

        let qualities = deobfuscate(&raw).expect("валидная строка");

        assert_eq!(qualities.len(), 1);
        assert_eq!(qualities[0].label, "720p");
        assert_eq!(qualities[0].url.as_str(), cdn_mp4("cdn.example.com", "720"));
    }

    #[test]
    fn url_with_comma_in_query_is_not_split() {
        let raw = format!(
            "[720p]https://cdn.example.com/720.mp4?ref=a,b&x=1,[1080p]{}",
            cdn_mp4("cdn.example.com", "1080")
        );

        let qualities = deobfuscate(&raw).expect("валидная строка");

        assert_eq!(qualities.len(), 2);
        assert_eq!(qualities[0].label, "720p");
        assert_eq!(
            qualities[0].url.as_str(),
            "https://cdn.example.com/720.mp4?ref=a,b&x=1"
        );
        assert_eq!(qualities[1].label, "1080p");
        assert_eq!(
            qualities[1].url.as_str(),
            cdn_mp4("cdn.example.com", "1080")
        );
    }

    // --- ошибки ---

    #[test]
    fn empty_and_whitespace_input() {
        assert!(matches!(deobfuscate(""), Err(KinoError::Deobfuscation(_))));
        assert!(matches!(
            deobfuscate("   \t\n"),
            Err(KinoError::Deobfuscation(_))
        ));
    }

    #[test]
    fn missing_closing_bracket() {
        assert!(matches!(
            deobfuscate("[720p no closing bracket"),
            Err(KinoError::Deobfuscation(_))
        ));
    }

    #[test]
    fn html_label_without_closing_span() {
        // Метка начинается с `<`, но закрывающего `</span>` нет.
        let raw = format!(
            "[<span class=\"pjs-prem-quality\">1080p Ultra]{}",
            cdn_mp4("cdn.example.com", "720")
        );
        assert!(matches!(
            deobfuscate(&raw),
            Err(KinoError::Deobfuscation(_))
        ));
    }

    #[test]
    fn unclosed_tag_missing_gt() {
        // `<span` без закрывающего `>`.
        let raw =
            "[<span class=\"pjs-prem-quality\"1080p Ultra</span]https://cdn.example.com/a.mp4";
        assert!(matches!(deobfuscate(raw), Err(KinoError::Deobfuscation(_))));
    }

    #[test]
    fn no_addresses() {
        assert!(matches!(
            deobfuscate("[720p]"),
            Err(KinoError::Deobfuscation(_))
        ));
        assert!(matches!(
            deobfuscate("[720p]   "),
            Err(KinoError::Deobfuscation(_))
        ));
    }

    #[test]
    fn empty_label_after_stripping_html() {
        let raw = "[<span class=\"pjs-prem-quality\"></span>]https://cdn.example.com/a.mp4";
        assert!(matches!(deobfuscate(raw), Err(KinoError::Deobfuscation(_))));
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
    fn base64_without_padding_works() {
        let plain = format!("[720p]{}", cdn_mp4("cdn.example.com", "720"));
        let raw = format!("#h{}", encode_no_pad(&plain));

        let qualities = deobfuscate(&raw).expect("base64 без padding");

        assert_eq!(qualities.len(), 1);
        assert_eq!(qualities[0].label, "720p");
        assert_eq!(qualities[0].url.as_str(), cdn_mp4("cdn.example.com", "720"));
    }

    #[test]
    fn each_junk_token_is_removed() {
        let plain = format!("[720p]{}", cdn_mp4("cdn.example.com", "720"));
        let b64 = encode(&plain);
        for token in JUNK_TOKENS {
            let raw = format!("#h{}", insert_at(&b64, &[(token, 7)]));
            let qualities = deobfuscate(&raw).expect("токен должен быть удалён");
            assert_eq!(qualities.len(), 1, "токен: {token}");
            assert_eq!(qualities[0].label, "720p");
            assert_eq!(qualities[0].url.as_str(), cdn_mp4("cdn.example.com", "720"));
        }
    }
}
