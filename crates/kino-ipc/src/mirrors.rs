//! Список зеркал, проверка, ручной выбор.

use futures_util::future::join;
use kino_api::{MirrorStore, SecretStore};
use kino_core::KinoError;
use url::Url;

use crate::dto::{same_mirror, MirrorDto, MirrorStateDto};
use crate::error::IpcError;
use crate::service::AppService;

impl<S: SecretStore, M: MirrorStore> AppService<S, M> {
    /// Известные зеркала и активное, без сети. Состояние всех `Unknown`.
    pub async fn mirrors(&self) -> Vec<MirrorDto> {
        let active = self.pool.store().active().await;
        let mut list: Vec<MirrorDto> = self
            .pool
            .config()
            .known
            .iter()
            .map(|url| MirrorDto::unknown(url, is_active(active.as_ref(), url)))
            .collect();
        if let Some(active) = active.as_ref() {
            if !list.iter().any(|mirror| mirror.active) {
                list.push(MirrorDto::unknown(active, true));
            }
        }
        list
    }

    /// Проба известных зеркал и standby. Активное зеркало вне списка известных (введённое
    /// вручную) проверяется отдельно, чтобы показать его состояние.
    pub async fn check_mirrors(&self) -> Vec<MirrorDto> {
        let (statuses, standby) = join(self.pool.health_check(), self.pool.probe_standby()).await;
        let active = self.pool.store().active().await;

        let standby = MirrorDto::from_status(&standby, active.as_ref(), true);
        let mut list: Vec<MirrorDto> = statuses
            .iter()
            .map(|status| MirrorDto::from_status(status, active.as_ref(), false))
            .collect();
        if let Some(active) = active.as_ref() {
            if !standby.active && !list.iter().any(|mirror| mirror.active) {
                list.push(self.check_manual(active).await);
            }
        }
        list.push(standby);
        list
    }

    /// Проверяет адрес и делает зеркало активным. Сессия сбрасывается: куки привязаны к домену.
    pub async fn add_mirror(&self, url: String) -> Result<MirrorDto, IpcError> {
        self.activate(&url).await
    }

    /// То же, что `add_mirror`: выбор зеркала проходит через проверку.
    pub async fn select_mirror(&self, url: String) -> Result<MirrorDto, IpcError> {
        self.activate(&url).await
    }

    async fn activate(&self, raw: &str) -> Result<MirrorDto, IpcError> {
        let url = parse_mirror_url(raw)?;
        let resolved = self.pool.add_manual(url).await?;
        self.set_session(None);
        self.reset_client()?;
        Ok(MirrorDto::from_resolved(&resolved))
    }

    async fn check_manual(&self, url: &Url) -> MirrorDto {
        match self.pool.add_manual(url.clone()).await {
            Ok(resolved) => MirrorDto::from_resolved(&resolved),
            Err(err) => {
                let state = if err == KinoError::Challenge {
                    MirrorStateDto::Challenge
                } else {
                    MirrorStateDto::Dead
                };
                MirrorDto {
                    state,
                    reason: Some(err.to_string()),
                    ..MirrorDto::unknown(url, true)
                }
            }
        }
    }
}

fn is_active(active: Option<&Url>, url: &Url) -> bool {
    active.is_some_and(|active| same_mirror(active, url))
}

/// Только http/https с хостом и без логина в адресе. Путь, запрос и фрагмент отбрасываются.
/// Введённый адрес в тексты ошибок не попадает: в нём могли оказаться посторонние данные.
fn parse_mirror_url(raw: &str) -> Result<Url, IpcError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(IpcError::invalid_input("Введите адрес зеркала"));
    }
    let mut url =
        Url::parse(trimmed).map_err(|_| IpcError::invalid_input("Некорректный адрес зеркала"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(IpcError::invalid_input(
            "Адрес зеркала должен начинаться с http:// или https://",
        ));
    }
    if url.host_str().is_none() {
        return Err(IpcError::invalid_input("В адресе зеркала нет домена"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(IpcError::invalid_input(
            "Адрес зеркала не должен содержать логин и пароль",
        ));
    }
    url.set_path("/");
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}
