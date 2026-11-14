//! Состояние приложения: HTTP-клиент, пул зеркал, хранилище секретов и текущая сессия.

use std::sync::{Mutex, PoisonError, RwLock};

use kino_api::{ClientConfig, HttpClient, MirrorConfig, MirrorPool, MirrorStore, SecretStore};
use kino_core::{Result, UserSession};

use crate::dto::UserDto;

/// Сервис под `tauri::command`. Пароль в полях не хранится: он приходит аргументом `login`
/// и сразу оборачивается в `Password`.
pub struct AppService<S: SecretStore, M: MirrorStore> {
    pub(crate) client: RwLock<HttpClient>,
    pub(crate) pool: MirrorPool<M>,
    pub(crate) secrets: S,
    pub(crate) session: Mutex<Option<UserSession>>,
}

impl<S: SecretStore, M: MirrorStore> AppService<S, M> {
    pub fn new(
        client_config: ClientConfig,
        secrets: S,
        mirror_store: M,
        mirror_config: MirrorConfig,
    ) -> Result<Self> {
        let client = HttpClient::new(client_config)?;
        let pool = MirrorPool::new(&client, mirror_store, mirror_config)?;
        Ok(Self {
            client: RwLock::new(client),
            pool,
            secrets,
            session: Mutex::new(None),
        })
    }

    /// Текущий пользователь; `None`, пока вход не выполнен.
    pub fn session(&self) -> Option<UserDto> {
        self.lock_session().as_ref().map(UserDto::from)
    }

    pub(crate) fn set_session(&self, session: Option<UserSession>) {
        *self.lock_session() = session;
    }

    fn lock_session(&self) -> std::sync::MutexGuard<'_, Option<UserSession>> {
        self.session.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Клон делит cookie-хранилище с оригиналом; замок не держится через `await`.
    pub(crate) fn client(&self) -> HttpClient {
        self.client
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// У `HttpClient` нет сброса cookie, поэтому вместо сброса строится новый клиент
    /// с теми же настройками.
    pub(crate) fn reset_client(&self) -> Result<()> {
        let fresh = HttpClient::new(self.client().config().clone())?;
        *self.client.write().unwrap_or_else(PoisonError::into_inner) = fresh;
        Ok(())
    }
}
