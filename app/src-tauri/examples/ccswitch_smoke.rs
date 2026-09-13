//! Real WebView2 smoke test against embedded production assets and a temporary DB.
//! cargo run --example ccswitch_smoke --features ccswitch-smoke
use std::sync::{Arc, Mutex};
use tauri::Emitter;

#[derive(Clone)]
struct SmokeResult(Arc<Mutex<Option<serde_json::Value>>>);

#[tauri::command]
fn smoke_report(
    app: tauri::AppHandle,
    state: tauri::State<'_, SmokeResult>,
    result: serde_json::Value,
) {
    *state.0.lock().unwrap() = Some(result);
    app.exit(0);
}

#[tauri::command]
fn bootstrap_public() -> serde_json::Value {
    serde_json::json!({"service_origin":"https://example.test", "health":{"status":"ok"}, "settings":{}})
}

#[tauri::command]
fn restore_session() -> Option<serde_json::Value> {
    None
}

const TEST_SCRIPT: &str = r#"
(() => {
  if (window.parent !== window) return;
  const invoke = (command, args) => window.__TAURI_INTERNALS__.invoke(command, args);
  const cc = (command, args) => invoke('plugin:molly-ccswitch|' + command, args);
  let finished = false;
  let started = false;
  let frame;
  const checks = [];
  const check = (condition, name) => { if (!condition) throw new Error(name); checks.push(name); };
  const report = result => {
    if (finished) return;
    finished = true;
    invoke('smoke_report', {result:{...result, checks}});
  };
  const diagnostics = () => ({origin:location.origin,frameUrl:frame?.contentWindow?.location.href,frameText:frame?.contentDocument?.body?.textContent?.slice(0,2000)});
  const waitFor = async (fn, label) => {
    for(let i=0;i<120;i++) { if(await fn()) return; await new Promise(resolve=>setTimeout(resolve,100)); }
    throw new Error(label);
  };
  setTimeout(() => report({ok:false,error:'Native CC Switch timeout',...diagnostics()}),45000);
  window.addEventListener('message', async event => {
    if (!frame || event.source !== frame.contentWindow || event.origin !== location.origin || event.data?.source !== 'molly-ccswitch' || finished) return;
    if (event.data.type === 'load-error') {
      try {
        check(window.__smokeInitFailure, 'Expected initialization failure');
        check(Boolean((await cc('get_init_error'))?.error), 'Module reports its initialization error');
        let denied = false;
        try { await cc('get_providers',{app:'codex'}); } catch { denied=true; }
        check(denied, 'Uninitialized module rejects data commands');
        check(Boolean((await invoke('bootstrap_public')).health), 'Host remains responsive');
        report({ok:true});
      } catch(error) { report({ok:false,error:String(error),...diagnostics()}); }
      return;
    }
    if (event.data.type !== 'ready' || started) return;
    started = true;
    try {
      const providers = await cc('get_providers',{app:'codex'});
      const provider = Object.values(providers).find(item=>item.name==='Molly Native Smoke');
      check(Boolean(provider), 'Rust DB-only import is visible through native IPC');
      check((await cc('get_current_provider',{app:'codex'})) !== provider.id, 'Import does not activate a provider');
      frame.contentWindow.postMessage({source:'mollycloud',type:'navigate',providerId:provider.id,app:'codex'},location.origin);
      await waitFor(()=>frame.contentDocument.querySelector('[data-provider-id="'+provider.id+'"]'), 'Provider card did not render');
      const card = frame.contentDocument.querySelector('[data-provider-id="'+provider.id+'"]');
      const activate = [...card.querySelectorAll('button')].find(button=>button.textContent.trim()==='启用');
      check(Boolean(activate), 'Original React card exposes explicit activation');
      activate.click();
      await waitFor(async()=>(await cc('get_current_provider',{app:'codex'}))===provider.id,'Activation did not finish');
      check(frame.contentDocument.body.textContent.includes('本机工具配置'), 'UI describes actual system targets');
      const live = await cc('read_live_provider_settings',{app:'codex'});
      check(Boolean(live?.config), 'Activation writes the actual tool configuration');
      const settings = await cc('get_settings');
      check(await cc('save_settings',{settings:{...settings,codexConfigDir:window.__smokeCustom}}), 'Custom tool directory can be saved');
      const resolved = await cc('get_config_dir',{app:'codex'});
      check(resolved.replaceAll('\\','/')===window.__smokeCustom.replaceAll('\\','/'), 'Backend resolves the custom tool directory');
      await cc('switch_provider',{app:'codex',id:provider.id});
      check(await cc('save_settings',{settings}), 'Default system directory can be restored');
      let denied = false;
      try { await cc('save_settings',{settings:{...settings,codexConfigDir:window.__smokeOutside}}); } catch { denied=true; }
      check(denied, 'Standalone manager data cannot become a tool configuration target');
      check(Array.isArray(await cc('list_sessions')), 'Session management is enabled');
      denied=false;
      try { await cc('restart_app'); } catch { denied=true; }
      check(denied,'Upstream lifecycle cannot restart MollyCloud');
      report({ok:true});
    } catch(error) { report({ok:false,error:String(error),...diagnostics()}); }
  });
  const start=()=>{
    frame=document.createElement('iframe');
    frame.style.cssText='position:fixed;inset:0;width:100%;height:100%;border:0;z-index:99999;background:white';
    frame.src='/ccswitch/index.html?embedded=1';
    document.body.appendChild(frame);
  };
  if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',start,{once:true});else start();
})();
"#;

fn main() {
    let failure_mode = std::env::args().any(|argument| argument == "--init-failure");
    let temp = tempfile::tempdir().expect("temporary smoke directory");
    let app_data = temp.path().join("molly");
    if failure_mode {
        std::fs::create_dir_all(&app_data).unwrap();
        std::fs::write(
            app_data.join("ccswitch"),
            b"fixture blocks private directory creation",
        )
        .unwrap();
    }
    let system_home = temp.path().join("system-user");
    let outside = system_home.join(".cc-switch");
    let custom = temp.path().join("custom-codex");
    std::fs::create_dir_all(&outside).unwrap();
    let sentinel = outside.join("config.json");
    std::fs::write(&sentinel, b"PROXY_MANAGED external sentinel").unwrap();
    let result = SmokeResult(Arc::new(Mutex::new(None)));
    let output = result.clone();
    let mut context = tauri::generate_context!();
    context.config_mut().identifier = "cn.mollycloud.ccswitch-smoke".into();
    context.config_mut().app.windows.clear();
    let webview_data = temp.path().join("webview");
    let check_private_home = app_data.join("ccswitch/home");
    let private_home = system_home.clone();
    let check_home = private_home.clone();
    let script = format!("window.__smokeOutside={};window.__smokeCustom={};window.__smokeInitFailure={failure_mode};\n{}", serde_json::to_string(&outside.to_string_lossy()).unwrap(), serde_json::to_string(&custom.to_string_lossy()).unwrap(), TEST_SCRIPT);
    let app = tauri::Builder::default()
        .manage(result)
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(molly_ccswitch::init_for_test(app_data, system_home))
        .invoke_handler(tauri::generate_handler![
            smoke_report,
            bootstrap_public,
            restore_session
        ])
        .setup(move |app| {
            if !failure_mode {
                molly_ccswitch::import_molly_provider(
                    app.handle(),
                    molly_ccswitch::MollyProviderImport {
                        account_id: "smoke-account".into(),
                        key_id: "smoke-key".into(),
                        name: "Molly Native Smoke".into(),
                        app: "codex".into(),
                        api_key: "sk-molly-smoke-not-a-real-key".into(),
                        base_url: "https://example.test/v1".into(),
                        model: "gpt-5.5".into(),
                        usage_script: None,
                    },
                )?;
                assert!(
                    !check_home.join(".codex/auth.json").exists(),
                    "first import wrote live auth"
                );
            }
            tauri::WebviewWindowBuilder::new(
                app,
                "console",
                tauri::WebviewUrl::App("index.html".into()),
            )
            .title("Molly CC Switch isolated verification")
            .visible(false)
            .inner_size(1180.0, 760.0)
            .data_directory(webview_data)
            .initialization_script(&script)
            .build()?;
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(55));
                // Independent watchdog only affects this test's own process.
                let _ = handle.emit("smoke-timeout", ());
                handle.exit(1);
            });
            Ok(())
        })
        .build(context)
        .expect("build native smoke host");
    app.run_return(|_, _| {});
    let result = output
        .0
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_else(|| serde_json::json!({"ok":false,"error":"native watchdog timed out"}));
    let external_unchanged =
        std::fs::read(&sentinel).unwrap() == b"PROXY_MANAGED external sentinel";
    // Upstream 3.20.3 projects third-party credentials into the provider table
    // in config.toml; auth.json is reserved for an optional native login.
    let live_config =
        std::fs::read_to_string(private_home.join(".codex/config.toml")).unwrap_or_default();
    let private_config_written = !live_config.is_empty();
    let legacy_unchanged = !check_private_home.join(".codex/config.toml").exists();
    let custom_written = custom.join("config.toml").exists();
    let private_credential_valid =
        molly_ccswitch::extract_codex_experimental_bearer_token(&live_config).as_deref()
            == Some("sk-molly-smoke-not-a-real-key");
    println!(
        "{}",
        serde_json::json!({"native":result,"custom_written":custom_written,"legacy_unchanged":legacy_unchanged,"init_failure_fixture":failure_mode,"external_unchanged":external_unchanged,"system_config_written":private_config_written,"system_credential_valid":private_credential_valid})
    );
    if result["ok"] != true
        || !legacy_unchanged || custom_written == failure_mode
        || !external_unchanged
        || private_config_written == failure_mode
        || private_credential_valid == failure_mode
    {
        std::process::exit(1);
    }
}
