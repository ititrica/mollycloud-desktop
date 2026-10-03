#[cfg(not(target_os = "macos"))]
pub use keyring::{Entry, Error};

#[cfg(target_os = "macos")]
mod local {
    use molly_local_secrets::SecretStore;
    use std::{path::PathBuf, sync::OnceLock};
    static STORE: OnceLock<SecretStore> = OnceLock::new();
    pub fn initialize(root: PathBuf) {
        STORE.get_or_init(|| SecretStore::new(root));
    }
    pub fn store() -> Result<&'static SecretStore, String> {
        STORE.get().ok_or_else(|| "应用私有存储尚未初始化".into())
    }
    #[derive(Debug)]
    pub enum Error {
        NoEntry,
        Storage,
    }
    pub struct Entry {
        slot: String,
    }
    impl Entry {
        pub fn new(service: &str, account: &str) -> Result<Self, Error> {
            Ok(Self {
                slot: format!("{service}:{account}"),
            })
        }
        pub fn get_password(&self) -> Result<String, Error> {
            let bytes = store()
                .map_err(|_| Error::Storage)?
                .read(&self.slot)
                .map_err(|_| Error::Storage)?
                .ok_or(Error::NoEntry)?;
            String::from_utf8(bytes).map_err(|_| Error::Storage)
        }
        pub fn set_password(&self, value: &str) -> Result<(), Error> {
            store()
                .map_err(|_| Error::Storage)?
                .write(&self.slot, value.as_bytes())
                .map_err(|_| Error::Storage)
        }
        pub fn delete_credential(&self) -> Result<(), Error> {
            store()
                .map_err(|_| Error::Storage)?
                .delete(&self.slot)
                .map_err(|_| Error::Storage)
        }
    }
}
#[cfg(target_os = "macos")]
pub use local::*;
