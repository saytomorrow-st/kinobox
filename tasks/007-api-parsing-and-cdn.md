---
id: 007
type: feature
status: open
model: deepseek-v4-pro
crate: kino-api
created: 2026-11-13
---

## Задача

`parse/` в `kino-api`: поиск, каталог и страница деталей из HTML, плюс реализация `KinoProvider::search`, `catalog` и `details` поверх `MirrorPool`, клиента и сессии. Потоки и серии идут отдельной задачей #013.

## Ограничения

- Адреса и селекторы только из `ARCHITECTURE.md`, из этого файла и из сохранённых страниц. Селекторы ниже взяты из исходников двух библиотек, на живых страницах подтверждены только помеченные пункты. Остальное сверять по фикстурам.
- Поиск: `GET /search/?do=search&subaction=search&q=<запрос>[&page=<n>]`. `POST /ajax/search/` в исходниках библиотек не найден.
- Карточка каталога: `div.b-content__inline_item` с атрибутами `data-id` и `data-url` (подтверждено на главной kvk.pub), название в `.b-content__inline_item-link a`, год, страна и жанр в тексте `div` рядом, постер `img[src]`, рейтинг `i.b-category-bestrating`, тип `i.entity`.
- Страница деталей: `input#post_id` (подтверждено), `og:url` с числовым id, `.b-post__title`, `.b-post__origtitle`, `.b-post__description_text`, таблица `.b-post__info` (годы, страны, жанры, длительность), переводы `#translators-list` с `data-translator_id` (подтверждено), сезоны `li.b-simple_season__item`, серии `li.b-simple_episode__item[data-season_id][data-episode_id]`.
- Фикстуры из публичных страниц, не более одной-двух на тип (главная, каталог, поиск, фильм, сериал). Фикстуры обезличить: убрать ник, email, `geo_ip`, куки, адреса потоков с подписями.
- Если вёрстка изменилась или результат пуст, вернуть `KinoError::Parse`, без паники.
- Файлы: `parse/search.rs`, `parse/catalog.rs`, `parse/details.rs`, `provider.rs`, `examples/capture.rs`, `CRATE.md`.

## Готово когда

- `cargo test -p kino-api` зелёный на фикстурах, сеть не используется
- Руками: `cargo run -p kino-api --example capture -- --out <каталог>` после входа из #012 сохраняет по одной обезличенной странице каждого типа
- Руками: `cargo run -p kino-api --example cli -- search "<запрос>"` печатает найденное, `details <url>` печатает карточку

## Заметки

Зависит от #012. Обезличенная страница фильма уже есть: `/private/var/folders/mx/4yhyy1ys09724dfwn_llns6r0000gn/T/opencode/sanitized/film_85038.html` (вне репозитория, нужна как первая фикстура). Прежняя версия задачи включала потоки и серии, они вынесены в #013.
