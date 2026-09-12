//! Uses an isolated credential name and synthetic tokens; never reads account credentials.
#[allow(dead_code)]
#[path = "../src/state.rs"]
mod state;

mod api {
    use serde_json::{json, Value};
    use std::sync::atomic::{AtomicUsize, Ordering};
    pub struct Sub2ApiClient { pub refreshes: AtomicUsize }
    impl Sub2ApiClient {
        pub fn new() -> Result<Self, String> { Ok(Self { refreshes: AtomicUsize::new(0) }) }
        pub async fn refresh(&self, _: &str) -> Result<Value, String> {
            self.refreshes.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(std::time::Duration::from_millis(40)).await;
            Ok(json!({ "access_token": "test-access", "refresh_token": "test-rotated", "expires_in": 3600 }))
        }
    }
}

#[tokio::test]
async fn console_and_pet_share_one_refresh() {
    let state = state::RuntimeState::new().unwrap();
    state.session.lock().await.refresh_token = Some("test-refresh".into());
    let (console, pet) = tokio::join!(state.access_token(), state.access_token());
    assert_eq!(console.unwrap(), "test-access");
    assert_eq!(pet.unwrap(), "test-access");
    assert_eq!(state.api.refreshes.load(std::sync::atomic::Ordering::SeqCst), 1);
    let session = state.session.lock().await;
    assert_eq!(session.refresh_token.as_deref(), Some("test-rotated"));
    assert!(!session.persist_refresh_token, "manual login remains memory-only");
}

const TEST_SERVICE: &str = "cn.mollycloud.tests.session-persistence";

#[test]
fn credential_child_process() {
    let Ok(account) = std::env::var("MOLLY_TEST_CREDENTIAL_ACCOUNT") else { return };
    let entry = keyring::Entry::new(TEST_SERVICE, &account).unwrap();
    assert_eq!(entry.get_password().unwrap(), "synthetic-refresh-before-restart");
    entry.set_password("synthetic-refresh-after-rotation").unwrap();
}

#[test]
fn windows_credentials_survive_process_restart_and_rotation() {
    let account = format!("isolated-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
    let entry = keyring::Entry::new(TEST_SERVICE, &account).unwrap();
    struct Cleanup(keyring::Entry);
    impl Drop for Cleanup { fn drop(&mut self) { let _ = self.0.delete_credential(); } }
    let _cleanup = Cleanup(keyring::Entry::new(TEST_SERVICE, &account).unwrap());
    entry.set_password("synthetic-refresh-before-restart").unwrap();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "credential_child_process", "--nocapture"])
        .env("MOLLY_TEST_CREDENTIAL_ACCOUNT", &account).output().unwrap();
    assert!(result.status.success(), "isolated child process failed: {}", String::from_utf8_lossy(&result.stdout));
    assert_eq!(keyring::Entry::new(TEST_SERVICE, &account).unwrap().get_password().unwrap(), "synthetic-refresh-after-rotation");
    entry.delete_credential().unwrap();
    assert!(matches!(entry.get_password(), Err(keyring::Error::NoEntry)));
}
