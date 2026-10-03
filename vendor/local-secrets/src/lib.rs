//! Application-owned encrypted files. No operating-system credential APIs.
use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng, Payload},
    Aes256Gcm, Nonce,
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

const HEADER: &[u8] = b"MOLLY-LOCAL-2\0";

#[derive(Clone)]
pub struct SecretStore {
    root: PathBuf,
}

impl SecretStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn entry_path(&self, slot: &str) -> PathBuf {
        self.root
            .join(format!("{:x}.bin", Sha256::digest(slot.as_bytes())))
    }

    fn prepare(&self) -> Result<(), String> {
        if self.root.exists() {
            regular_directory(&self.root)?;
        } else {
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                fs::DirBuilder::new()
                    .recursive(true)
                    .mode(0o700)
                    .create(&self.root)
                    .map_err(storage_error)?;
            }
            #[cfg(not(unix))]
            fs::create_dir_all(&self.root).map_err(storage_error)?;
        }
        regular_directory(&self.root)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.root, fs::Permissions::from_mode(0o700))
                .map_err(storage_error)?;
        }
        Ok(())
    }

    fn key(&self, create: bool) -> Result<Vec<u8>, String> {
        let path = self.root.join("master.key");
        if !path.exists() && create {
            self.prepare()?;
            let key = Aes256Gcm::generate_key(&mut OsRng);
            let mut file = tempfile::NamedTempFile::new_in(&self.root).map_err(storage_error)?;
            private_file(file.path())?;
            file.write_all(&key).map_err(storage_error)?;
            file.as_file().sync_all().map_err(storage_error)?;
            match file.persist_noclobber(&path) {
                Ok(_) => (),
                Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => (),
                Err(error) => return Err(storage_error(error.error)),
            }
        }
        regular_directory(&self.root)?;
        regular_file(&path)?;
        let key = fs::read(&path).map_err(storage_error)?;
        if key.len() != 32 {
            return Err("应用内加密密钥已损坏".into());
        }
        Ok(key)
    }

    fn encrypt(&self, data: &[u8], aad: &[u8]) -> Result<Vec<u8>, String> {
        let cipher =
            Aes256Gcm::new_from_slice(&self.key(true)?).map_err(|_| "应用内加密密钥无效")?;
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let encrypted = cipher
            .encrypt(&nonce, Payload { msg: data, aad })
            .map_err(|_| "应用内数据加密失败")?;
        Ok([HEADER, nonce.as_slice(), &encrypted].concat())
    }

    fn decrypt(&self, data: &[u8], aad: &[u8]) -> Result<Vec<u8>, String> {
        let body = data
            .strip_prefix(HEADER)
            .filter(|v| v.len() >= 28)
            .ok_or("应用内加密数据格式无效，请重新保存")?;
        let cipher =
            Aes256Gcm::new_from_slice(&self.key(false)?).map_err(|_| "应用内加密密钥无效")?;
        cipher
            .decrypt(
                Nonce::from_slice(&body[..12]),
                Payload {
                    msg: &body[12..],
                    aad,
                },
            )
            .map_err(|_| "应用内数据解密失败，请重新保存".into())
    }

    pub fn seal(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        self.encrypt(data, b"molly-payload")
    }
    pub fn unseal(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        self.decrypt(data, b"molly-payload")
    }

    pub fn read(&self, slot: &str) -> Result<Option<Vec<u8>>, String> {
        let path = self.entry_path(slot);
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(storage_error(error)),
            Ok(_) => (),
        }
        regular_directory(&self.root)?;
        regular_file(&path)?;
        self.decrypt(&fs::read(path).map_err(storage_error)?, slot.as_bytes())
            .map(Some)
    }

    pub fn write(&self, slot: &str, data: &[u8]) -> Result<(), String> {
        self.prepare()?;
        let encrypted = self.encrypt(data, slot.as_bytes())?;
        let mut file = tempfile::NamedTempFile::new_in(&self.root).map_err(storage_error)?;
        private_file(file.path())?;
        file.write_all(&encrypted).map_err(storage_error)?;
        file.as_file().sync_all().map_err(storage_error)?;
        file.persist(self.entry_path(slot))
            .map_err(|e| storage_error(e.error))?;
        Ok(())
    }

    pub fn delete(&self, slot: &str) -> Result<(), String> {
        if !self.root.exists() {
            return Ok(());
        }
        regular_directory(&self.root)?;
        match fs::remove_file(self.entry_path(slot)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(storage_error(error)),
        }
    }
}

fn storage_error(_: std::io::Error) -> String {
    "无法访问应用私有加密存储，请检查目录权限".into()
}
fn regular_directory(path: &Path) -> Result<(), String> {
    if fs::symlink_metadata(path)
        .map_err(storage_error)?
        .file_type()
        .is_dir()
    {
        Ok(())
    } else {
        Err("应用私有存储不能是符号链接或普通文件".into())
    }
}
fn regular_file(path: &Path) -> Result<(), String> {
    if fs::symlink_metadata(path)
        .map_err(storage_error)?
        .file_type()
        .is_file()
    {
        private_file(path)
    } else {
        Err("应用加密文件不能是符号链接或目录".into())
    }
}
fn private_file(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(storage_error)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn persistent_encrypted_entries_survive_rotation_and_delete() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("private");
        let store = SecretStore::new(&root);
        assert_eq!(store.read("session").unwrap(), None);
        assert!(
            !root.exists(),
            "reading absent credentials must not create files"
        );
        store.write("session", b"synthetic-password").unwrap();
        let bytes = fs::read(store.entry_path("session")).unwrap();
        assert!(!bytes.windows(18).any(|s| s == b"synthetic-password"));
        let restarted = SecretStore::new(&root);
        assert_eq!(
            restarted.read("session").unwrap().unwrap(),
            b"synthetic-password"
        );
        restarted.write("session", b"rotated").unwrap();
        assert_eq!(store.read("session").unwrap().unwrap(), b"rotated");
        restarted.delete("session").unwrap();
        assert_eq!(store.read("session").unwrap(), None);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&root).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(root.join("master.key"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
    #[test]
    fn rejects_tampering_and_cross_slot_substitution() {
        let dir = tempfile::tempdir().unwrap();
        let store = SecretStore::new(dir.path());
        store.write("one", b"test").unwrap();
        fs::copy(store.entry_path("one"), store.entry_path("two")).unwrap();
        assert!(store.read("two").is_err());
        let mut bytes = fs::read(store.entry_path("one")).unwrap();
        *bytes.last_mut().unwrap() ^= 1;
        fs::write(store.entry_path("one"), bytes).unwrap();
        assert!(store.read("one").is_err());
    }
    #[test]
    fn concurrent_first_writes_share_one_master_key() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("private");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(6));
        let workers: Vec<_> = (0..6)
            .map(|index| {
                let store = SecretStore::new(&root);
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    store
                        .write(&format!("slot-{index}"), b"synthetic-value")
                        .unwrap();
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }
        let store = SecretStore::new(root);
        for index in 0..6 {
            assert_eq!(
                store.read(&format!("slot-{index}")).unwrap().unwrap(),
                b"synthetic-value"
            );
        }
    }
    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_private_directory() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("link");
        std::os::unix::fs::symlink(dir.path(), &path).unwrap();
        assert!(SecretStore::new(path).write("test", b"secret").is_err());
    }
}
