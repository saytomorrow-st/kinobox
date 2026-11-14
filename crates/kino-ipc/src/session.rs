//! Запуск, вход и выход.

use kino_api::{
    login as site_login, login_and_remember, Credentials, MirrorStore, Password, SecretStore,
};
use kino_core::KinoError;
use url::Url;

use crate::dto::{BootState, UserDto};
use crate::error::IpcError;
use crate::service::AppService;

const NO_MIRRORS_MESSAGE: &str = "Ни одно зеркало не отвечает или все требуют проверку браузера \
     (Anubis). Проверьте подключение или добавьте зеркало вручную.";
const CHALLENGE_MESSAGE: &str = "Зеркало требует проверку браузера (Anubis), приложение её не \
     проходит. Выберите другое зеркало.";

impl<S: SecretStore, M: MirrorStore> AppService<S, M> {
    /// Тихий вход сохранёнными данными. Данные в Keychain не меняются ни при каком исходе.
    pub async fn boot(&self) -> BootState {
        self.set_session(None);

        let credentials = match self.secrets.load().await {
            Ok(Some(credentials)) => credentials,
            Ok(None) => return BootState::NeedsLogin { email: None },
            Err(err) => return unavailable(&err),
        };
        let mirror = match self.pool.resolve().await {
            Ok(mirror) => mirror.url,
            Err(err) => return unavailable(&err),
        };

        // `login` вместо `ensure_logged_in`: данные уже загружены, второе чтение Keychain не нужно.
        match site_login(&self.client(), &mirror, &credentials).await {
            Ok(session) => {
                let user = UserDto::from(&session);
                self.set_session(Some(session));
                BootState::SignedIn { user }
            }
            Err(KinoError::LoginRejected(_)) => BootState::NeedsLogin {
                email: Some(credentials.email),
            },
            Err(err) => unavailable(&err),
        }
    }

    /// Вход введёнными данными. В Keychain они попадают только после успеха.
    pub async fn login(&self, email: String, password: String) -> Result<UserDto, IpcError> {
        let password = Password::new(password);
        let email = email.trim().to_string();
        if email.is_empty() {
            return Err(IpcError::invalid_input("Введите email"));
        }
        if password.is_empty() {
            return Err(IpcError::invalid_input("Введите пароль"));
        }

        let mirror = self.mirror_url().await?;
        let credentials = Credentials { email, password };
        let session =
            login_and_remember(&self.client(), &mirror, &self.secrets, &credentials).await?;
        let user = UserDto::from(&session);
        self.set_session(Some(session));
        Ok(user)
    }

    /// Удаляет данные из Keychain, затем сбрасывает сессию и cookie клиента.
    /// Если Keychain вернул ошибку, сессия остаётся.
    pub async fn logout(&self) -> Result<(), IpcError> {
        self.secrets.clear().await?;
        self.set_session(None);
        self.reset_client()?;
        Ok(())
    }

    async fn mirror_url(&self) -> Result<Url, IpcError> {
        match self.pool.store().active().await {
            Some(url) => Ok(url),
            None => Ok(self.pool.resolve().await?.url),
        }
    }
}

fn unavailable(err: &KinoError) -> BootState {
    let message = match err {
        KinoError::AllMirrorsUnavailable => NO_MIRRORS_MESSAGE.to_string(),
        KinoError::Challenge => CHALLENGE_MESSAGE.to_string(),
        other => other.to_string(),
    };
    BootState::Unavailable { message }
}
