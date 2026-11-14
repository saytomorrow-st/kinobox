//! Обезличивание страницы HDrezka перед сохранением фикстуры.
//!
//! Цель — безопасно отдать заказчику публичную страницу без ников, токенов,
//! email и подписанных ссылок на потоки. Замены точечные, под известную
//! вёрстку HDrezka; это не универсальный скруббер персональных данных.

/// Нейтральное значение, которым заменяется адрес почты.
const NEUTRAL_EMAIL: &str = "user@example.com";

/// Значения скрытых полей формы и переменных скриптов -> нейтральные.
const INPUT_FIELDS: [(&str, &str); 5] = [
    ("member_user_id", "0"),
    ("member_user_token", "0000"),
    ("ctrl_token_id", "0000"),
    ("ctrl_favs", "00000000-0000-0000-0000-000000000000"),
    ("name", "user"), // ник в форме комментария (`name="name"`)
];

/// Переменные/поля в скриптах и JSON-объектах -> нейтральные.
const JS_FIELDS: [(&str, &str); 3] = [
    ("dle_login_hash", "0000"),
    ("profileId", "0"),
    ("geo_ip", "0.0.0.0"),
];

/// Обезличивает страницу: заменяет значения перечисленных полей, адреса
/// почты, ссылки на потоки и превью. Структура HTML сохраняется.
pub fn sanitize_page(html: &str) -> String {
    let mut out = html.to_string();
    for (key, replacement) in INPUT_FIELDS {
        out = redact_input_field(&out, key, replacement);
    }
    for (key, replacement) in JS_FIELDS {
        out = redact_quoted(&out, key, replacement);
    }
    out = redact_quoted(&out, "streams", "[360p]https://cdn.example.com/s/x.mp4");
    out = redact_quoted(&out, "thumbnails", "/ajax/get_cdn_tiles/0/0/");
    out = redact_emails(&out);
    out
}

/// Есть ли в тексте адрес почты, отличный от обезличенного `user@example.com`.
pub fn contains_email(html: &str) -> bool {
    let mut rest = html;
    while let Some((start, end)) = find_email(rest) {
        if &rest[start..end] != NEUTRAL_EMAIL {
            return true;
        }
        rest = &rest[end..];
    }
    false
}

/// Заменяет `value="..."` у `<input>`, чей `name` или `id` равен `key`.
fn redact_input_field(html: &str, key: &str, replacement: &str) -> String {
    let mut out = String::new();
    let mut rest = html;
    loop {
        let Some(open) = rest.find("<input") else {
            out.push_str(rest);
            break;
        };
        let Some(close) = rest.get(open..).and_then(|tail| tail.find('>')) else {
            out.push_str(rest);
            break;
        };
        let tag_end = open + close + 1;
        let tag = &rest[open..tag_end];
        if input_matches(tag, key) {
            out.push_str(&rest[..open]);
            out.push_str(&replace_attr(tag, "value", replacement));
        } else {
            out.push_str(&rest[..tag_end]);
        }
        rest = &rest[tag_end..];
    }
    out
}

/// Есть ли у тега `<input>` атрибут `name` или `id` со значением `key`.
fn input_matches(tag: &str, key: &str) -> bool {
    has_attr_value(tag, "name", key) || has_attr_value(tag, "id", key)
}

/// Есть ли атрибут `attr="want"` или `attr='want'`.
fn has_attr_value(tag: &str, attr: &str, want: &str) -> bool {
    for quote in ['"', '\''] {
        let pattern = format!("{attr}={quote}");
        let mut from = 0;
        while let Some(pos) = tag.get(from..).and_then(|tail| tail.find(&pattern)) {
            let start = from + pos + pattern.len();
            let Some(rest) = tag.get(start..) else {
                break;
            };
            let Some(end) = rest.find(quote) else {
                break;
            };
            if &rest[..end] == want {
                return true;
            }
            from = start + end + 1;
        }
    }
    false
}

/// Заменяет первое `attr="..."` (или `attr='...'`) на `attr="replacement"`.
fn replace_attr(tag: &str, attr: &str, replacement: &str) -> String {
    for quote in ['"', '\''] {
        let pattern = format!("{attr}={quote}");
        let Some(pos) = tag.find(&pattern) else {
            continue;
        };
        let start = pos + pattern.len();
        let Some(rest) = tag.get(start..) else {
            continue;
        };
        let Some(end) = rest.find(quote) else {
            continue;
        };
        let mut out = String::with_capacity(tag.len());
        out.push_str(&tag[..start]);
        out.push_str(replacement);
        out.push_str(&tag[start + end..]);
        return out;
    }
    tag.to_string()
}

/// Заменяет значение после ключа в форматах `key = 'v'`, `key: 'v'`, `"key":"v"`.
fn redact_quoted(html: &str, key: &str, replacement: &str) -> String {
    let mut out = String::new();
    let mut rest = html;
    loop {
        let Some(pos) = find_word(rest, key) else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..pos + key.len()]);
        let tail = &rest[pos + key.len()..];
        match value_span_after(tail) {
            Some((start, end, _)) => {
                out.push_str(&tail[..start]);
                out.push_str(replacement);
                rest = &tail[end..];
            }
            None => {
                out.push_str(tail);
                break;
            }
        }
    }
    out
}

/// Позиция вхождения `needle` как отдельного слова (границы — не буква/цифра/`_`).
fn find_word(haystack: &str, needle: &str) -> Option<usize> {
    let bytes = haystack.as_bytes();
    let mut from = 0;
    while let Some(pos) = haystack.get(from..).and_then(|tail| tail.find(needle)) {
        let abs = from + pos;
        let before_ok = abs == 0 || !is_word_char(bytes[abs - 1]);
        let after = abs + needle.len();
        let after_ok = after >= bytes.len() || !is_word_char(bytes[after]);
        if before_ok && after_ok {
            return Some(abs);
        }
        from = abs + needle.len();
    }
    None
}

fn is_word_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Если после ключа идёт `=` или `:` (возможно с пробелами) и строка в кавычках,
/// возвращает (начало значения, индекс закрывающей кавычки, символ кавычки).
/// Понимает и JS-форму `key: 'v'`, и JSON-форму `"key":"v"` (закрывающая
/// кавычка ключа опциональна).
fn value_span_after(tail: &str) -> Option<(usize, usize, char)> {
    let bytes = tail.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    // Закрывающая кавычка ключа в JSON: `"key":"v"`.
    if i < bytes.len() && matches!(bytes[i], b'"' | b'\'') {
        i += 1;
    }
    if i >= bytes.len() || !matches!(bytes[i], b'=' | b':') {
        return None;
    }
    i += 1;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    let quote = match bytes.get(i)? {
        b'"' => b'"',
        b'\'' => b'\'',
        _ => return None,
    };
    i += 1;
    let value_start = i;
    while i < bytes.len() && bytes[i] != quote {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    Some((value_start, i, quote as char))
}

/// Заменяет адреса почты на `user@example.com`.
fn redact_emails(html: &str) -> String {
    let mut out = String::new();
    let mut rest = html;
    while let Some((start, end)) = find_email(rest) {
        out.push_str(&rest[..start]);
        out.push_str(NEUTRAL_EMAIL);
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// Первый адрес почты в `html` как диапазон байтов, иначе `None`.
fn find_email(html: &str) -> Option<(usize, usize)> {
    let bytes = html.as_bytes();
    let at = html.find('@')?;
    let mut start = at;
    while start > 0 && is_email_char(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = at + 1;
    while end < bytes.len() && is_email_char(bytes[end]) {
        end += 1;
    }
    if start == at || end == at + 1 {
        // пустая локальная часть или пустой домен
        return None;
    }
    let domain = &html[at + 1..end];
    if !domain.contains('.') {
        return None;
    }
    Some((start, end))
}

fn is_email_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'%' | b'+' | b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_hidden_input_values() {
        let html = r#"<input type="hidden" id="member_user_id" name="member_user_id" value="123456" />
        <input type="hidden" id="member_user_token" name="member_user_token" value="secret-token" />
        <input type="hidden" id="ctrl_favs" value="aaaa-bbbb-cccc" />
        <input type="hidden" id="ctrl_token_id" value="tok123" />"#;
        let cleaned = sanitize_page(html);
        assert!(cleaned.contains("value=\"0\""), "{cleaned}");
        assert!(cleaned.contains("value=\"0000\""), "{cleaned}");
        assert!(
            cleaned.contains("value=\"00000000-0000-0000-0000-000000000000\""),
            "{cleaned}"
        );
        assert!(!cleaned.contains("secret-token"), "{cleaned}");
        assert!(!cleaned.contains("tok123"), "{cleaned}");
        assert!(!cleaned.contains("aaaa-bbbb-cccc"), "{cleaned}");
    }

    #[test]
    fn replaces_comment_name_field() {
        let html = r#"<input type="hidden" name="name" id="name" value="kino_lover" />"#;
        let cleaned = sanitize_page(html);
        assert!(cleaned.contains("value=\"user\""), "{cleaned}");
        assert!(!cleaned.contains("kino_lover"), "{cleaned}");
    }

    #[test]
    fn replaces_script_and_json_fields() {
        let html = r#"<script>var dle_login_hash = 'abc123', dle_group = 4;</script>
        <script>window.op('identify', { profileId: '999', email: 'real@example.com' });</script>
        <script>var cfg = {"geo_ip":"8.8.8.8","geo_iso":"fi"};</script>"#;
        let cleaned = sanitize_page(html);
        assert!(cleaned.contains("dle_login_hash = '0000'"), "{cleaned}");
        assert!(cleaned.contains("profileId: '0'"), "{cleaned}");
        assert!(cleaned.contains("geo_ip\":\"0.0.0.0\""), "{cleaned}");
        assert!(!cleaned.contains("abc123"), "{cleaned}");
        assert!(!cleaned.contains("8.8.8.8"), "{cleaned}");
    }

    #[test]
    fn replaces_emails() {
        let html = "пишите на a.b-c+tag@example.org и user@example.com";
        let cleaned = sanitize_page(html);
        assert!(!cleaned.contains("a.b-c+tag@example.org"), "{cleaned}");
        assert!(cleaned.contains("user@example.com"), "{cleaned}");
    }

    #[test]
    fn replaces_streams_and_thumbnails() {
        let html = r#"{"streams":"[360p]https:\/\/signed.example\/x.mp4","thumbnails":"\/ajax\/get_cdn_tiles\/85038\/111\/"}"#;
        let cleaned = sanitize_page(html);
        assert!(
            cleaned.contains("https://cdn.example.com/s/x.mp4"),
            "{cleaned}"
        );
        assert!(cleaned.contains("/ajax/get_cdn_tiles/0/0/"), "{cleaned}");
        assert!(!cleaned.contains("signed.example"), "{cleaned}");
    }

    #[test]
    fn keeps_html_structure() {
        let html = r#"<div class="b-post__title"><h1>Одиссея</h1></div>
        <input type="hidden" name="post_id" id="post_id" value="85038" />"#;
        let cleaned = sanitize_page(html);
        assert!(
            cleaned.contains("<div class=\"b-post__title\"><h1>Одиссея</h1></div>"),
            "{cleaned}"
        );
        assert!(cleaned.contains("value=\"85038\""), "{cleaned}");
    }

    #[test]
    fn contains_email_ignores_neutral_placeholder() {
        assert!(!contains_email("mail: user@example.com"));
        assert!(contains_email("mail: real@example.com"));
        assert!(contains_email("a@b.com и user@example.com"));
        assert!(!contains_email("нет почты, просто @media и 1@2"));
    }
}
