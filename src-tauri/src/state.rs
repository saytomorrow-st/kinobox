use kino_ipc::{AppService, ClientConfig, KeychainSecretStore, MemoryMirrorStore, MirrorConfig};

pub type Service = AppService<KeychainSecretStore, MemoryMirrorStore>;

/// Сеть и Keychain здесь не трогаются: первое обращение делает `boot`.
pub fn create_service() -> Result<Service, Box<dyn std::error::Error>> {
    let service = AppService::new(
        ClientConfig::default(),
        KeychainSecretStore::new(),
        MemoryMirrorStore::new(),
        MirrorConfig::new()?,
    )?;
    Ok(service)
}
