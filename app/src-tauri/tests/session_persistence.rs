//! Uses an isolated credential name and synthetic tokens; never reads account credentials.
#[path = "../src/credential_store.rs"]
mod credential_store;
#[allow(dead_code)]
#[path = "../src/state.rs"]
mod state;

mod api {
    use serde_json::{json, Value};
    use std::sync::atomic::{AtomicUsize, Ordering};
    pub struct Sub2ApiClient {
        pub refreshes: AtomicUsize,
    }
    impl Sub2ApiClient {
        pub fn new() -> Result<Self, String> {
            Ok(Self {
                refreshes: AtomicUsize::new(0),
            })
        }
        pub async fn refresh(&self, _: &str) -> Result<Value, String> {
            self.refreshes.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(std::time::Duration::from_millis(40)).await;
            Ok(
                json!({ "access_token": "test-access", "refresh_token": "test-rotated", "expires_in": 3600 }),
            )
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
    assert_eq!(
        state
            .api
            .refreshes
            .load(std::sync::atomic::Ordering::SeqCst),
        1
    );
    let session = state.session.lock().await;
    assert_eq!(session.refresh_token.as_deref(), Some("test-rotated"));
    assert!(
        !session.persist_refresh_token,
        "manual login remains memory-only"
    );
}

#[cfg(target_os = "macos")]
fn test_store() -> &'static tempfile::TempDir {
    static DIR: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
    let dir = DIR.get_or_init(|| tempfile::tempdir().unwrap());
    credential_store::initialize(dir.path().join("credentials-v2"));
    dir
}

#[cfg(target_os = "macos")]
#[test]
fn credential_child_process() {
    let Ok(root) = std::env::var("MOLLY_TEST_CREDENTIAL_ROOT") else {
        return;
    };
    credential_store::initialize(std::path::PathBuf::from(root));
    let entry = credential_store::Entry::new("synthetic-service", "synthetic-account").unwrap();
    assert_eq!(
        entry.get_password().unwrap(),
        "synthetic-refresh-before-restart"
    );
    entry
        .set_password("synthetic-refresh-after-rotation")
        .unwrap();
}

#[cfg(target_os = "macos")]
#[test]
fn app_credentials_survive_process_restart_and_rotation() {
    let dir = test_store();
    let entry = credential_store::Entry::new("synthetic-service", "synthetic-account").unwrap();
    entry
        .set_password("synthetic-refresh-before-restart")
        .unwrap();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "credential_child_process", "--nocapture"])
        .env(
            "MOLLY_TEST_CREDENTIAL_ROOT",
            dir.path().join("credentials-v2"),
        )
        .output()
        .unwrap();
    assert!(result.status.success(), "isolated child process failed");
    assert_eq!(
        entry.get_password().unwrap(),
        "synthetic-refresh-after-rotation"
    );
    entry.delete_credential().unwrap();
    assert!(matches!(
        entry.get_password(),
        Err(credential_store::Error::NoEntry)
    ));
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn auto_login_credentials_persist_after_verification_and_clear_on_logout() {
    test_store();
    let state = state::RuntimeState::new().unwrap();
    state
        .remember_login_credentials("synthetic@example.test", "synthetic-password")
        .await;
    assert!(
        state::load_login_credentials().unwrap().is_none(),
        "pending two-factor login stays in memory"
    );
    let data = serde_json::json!({"access_token":"synthetic-access", "refresh_token":"synthetic-refresh", "user":{"id":1}});
    state.accept_login(&data, true).await.unwrap();
    let saved = state::load_login_credentials().unwrap().unwrap();
    assert_eq!(saved.email, "synthetic@example.test");
    assert_eq!(saved.password, "synthetic-password");
    // Token rotation must retain the saved email/password.
    state.session.lock().await.expires_at = None;
    state.access_token().await.unwrap();
    assert_eq!(state::load_refresh_token().unwrap(), "test-rotated");
    assert_eq!(
        state::load_login_credentials().unwrap().unwrap().password,
        "synthetic-password"
    );
    state.clear_session().await;
    assert!(state::load_login_credentials().unwrap().is_none());
    assert!(state::load_refresh_token().is_err());
    state
        .remember_login_credentials("manual@example.test", "synthetic-manual-password")
        .await;
    state.accept_login(&data, false).await.unwrap();
    assert!(state::load_login_credentials().unwrap().is_none());
    assert!(state::load_refresh_token().is_err());
}
