//! Native WebView smoke test against embedded production assets and a temporary DB.
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

// This command is exclusive to the isolated smoke executable; no real account
// or credentials are consulted by the test window.
#[tauri::command]
fn sync_molly_key_providers(app: tauri::AppHandle, agent: String) -> Result<serde_json::Value,String> {
    molly_ccswitch::ensure_molly_official(&app,&agent)?;
    let saved = molly_ccswitch::find_molly_key_provider(&app,"smoke-account","smoke-key",&agent)?;
    Ok(serde_json::json!({"agent":agent,"current_removed":false,"keys":[{
        "id":"smoke-key","name":if agent=="mcode" {"Molly MiniMax Smoke"} else {"Molly Native Smoke"},
        "key":"sk-••••test","status":"active","group":{"name":"Fixture","platform":"openai"},
        "quota":0,"quota_used":0,"usage":null,"compatible":true,"error":null,
        "provider_id":saved.as_ref().map(|(id,_)|id),"model":saved.map(|(_,model)|model).unwrap_or_default()
    }]}))
}

#[tauri::command]
fn prepare_smoke_opencode(app: tauri::AppHandle) -> Result<String,String> {
    molly_ccswitch::import_molly_provider(&app,molly_ccswitch::MollyProviderImport{
        account_id:"smoke-account".into(),key_id:"smoke-key".into(),name:"Molly Native Smoke".into(),app:"opencode".into(),
        api_key:"sk-molly-smoke-not-a-real-key".into(),base_url:"https://example.test/v1".into(),model:"test-model".into(),usage_script:None
    })
}

#[tauri::command]
fn set_molly_key_mode(app: tauri::AppHandle, agent: String, key_id: String, mode: String) -> Result<(),String> {
    if agent != "codex" || key_id != "smoke-key" { return Err("Invalid fixture".into()); }
    let models = vec!["gpt-6.1-sol-fast".into(), "gpt-6-sol".into(), "gpt-6-sol-unavailable-alias".into(), "other-model".into()];
    molly_ccswitch::update_molly_codex_provider(&app, "smoke-account", "smoke-key", Some(&mode), (mode == "mapped").then_some(models.as_slice()))
}

#[tauri::command]
fn smoke_catalog_matches() -> Result<bool,String> {
    let directory = molly_ccswitch::cli_config_dir("codex")?;
    let bytes = std::fs::read(directory.join("cc-switch-model-catalog.json")).map_err(|e| e.to_string())?;
    let catalog: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let rows = catalog["models"].as_array().ok_or("Catalog missing models")?;
    Ok(rows.len() == 2 && rows.iter().all(|row| {
        let efforts:Vec<_> = row["supported_reasoning_levels"].as_array().into_iter().flatten().filter_map(|entry| entry["effort"].as_str()).collect();
        row["context_window"] == 1050000 && row["max_context_window"] == 1050000 && efforts == ["low","medium","high","xhigh","max","ultra"]
    }))
}

const TEST_SCRIPT: &str = r#"
(() => {
  if (window.parent !== window) return;
  const invoke = (command, args) => window.__TAURI_INTERNALS__.invoke(command, args);
  const cc = (command, args) => invoke('plugin:molly-ccswitch|' + command, args);
  let finished = false;
  let started = false;
  let frame;
  let manualOpenCodeRequest=false;
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
    if(event.data.type==='key-action'&&event.data.app==='opencode'&&event.data.action==='configure-enable') {manualOpenCodeRequest=true;return;}
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
      await invoke('sync_molly_key_providers',{agent:'codex'});
      check((await cc('get_current_provider',{app:'codex'})) === 'codex-official', 'First empty Official selection does not activate an account key');
      frame.contentWindow.postMessage({source:'mollycloud',type:'navigate',providerId:provider.id,app:'codex'},location.origin);
      await waitFor(()=>frame.contentDocument.querySelector('[data-provider-id="'+provider.id+'"]'), 'Provider card did not render');
      const card = frame.contentDocument.querySelector('[data-provider-id="'+provider.id+'"]');
      const activate = [...card.querySelectorAll('button')].find(button=>button.textContent.trim()==='启用');
      check(Boolean(activate), 'Unified account key exposes explicit activation');
      activate.click();
      await waitFor(async()=>(await cc('get_current_provider',{app:'codex'}))===provider.id,'Activation did not finish');
      await invoke('sync_molly_key_providers',{agent:'codex'});
      check((await cc('get_current_provider',{app:'codex'}))===provider.id,'Repeated sync preserves the explicitly selected key');
      check(frame.contentDocument.body.textContent.includes('本机工具配置'), 'UI describes actual system targets');
      const live = await cc('read_live_provider_settings',{app:'codex'});
      check(Boolean(live?.config), 'Activation writes the actual tool configuration');
      check(live.config.includes('model_context_window = 1050000'), 'Molly Codex defaults to one million context');
      card.querySelector('[data-key-action="configure"]').click();
      await waitFor(()=>frame.contentDocument.querySelector('#provider-form'), 'Full provider editor did not open');
      check([...frame.contentDocument.querySelectorAll('#provider-form label')].some(label=>label.textContent.includes('1M') && label.querySelector('input')?.checked), 'Full editor displays enabled 1M context');
      check([...frame.contentDocument.querySelectorAll('h2')].some(el=>el.textContent.includes('编辑供应商')), 'Edit icon opens the original full provider editor');
      const modelInput=frame.contentDocument.querySelector('#codexDefaultModel');
      check(Boolean(modelInput), 'Full provider editor exposes model editing');
      Object.getOwnPropertyDescriptor(frame.contentWindow.HTMLInputElement.prototype,'value').set.call(modelInput,'gpt-5.5-smoke-edit');
      modelInput.dispatchEvent(new frame.contentWindow.Event('input',{bubbles:true}));
      frame.contentDocument.querySelector('button[form="provider-form"]').click();
      await waitFor(()=>!frame.contentDocument.querySelector('#provider-form'), 'Full editor did not close');
      await waitFor(()=>frame.contentDocument.querySelector('[data-key-id="smoke-key"]').textContent.includes('gpt-5.5-smoke-edit'),'Edited model is immediately reflected in account key row');
      const editedLive = await cc('read_live_provider_settings',{app:'codex'});
      check(editedLive.config.includes('gpt-5.5-smoke-edit'),'Full editor saves through original provider update');
      await waitFor(()=>frame.contentDocument.querySelector('[data-key-id="smoke-key"] [role="switch"]'), 'Per-key mode switch absent');
      frame.contentDocument.querySelector('[data-key-id="smoke-key"] [role="switch"]').click();
      await waitFor(async()=>(await cc('get_providers',{app:'codex'}))[provider.id].meta.mollyCodexMode==='mapped','Mode did not persist');
      const mapped = (await cc('get_providers',{app:'codex'}))[provider.id];
      check(mapped.settingsConfig.modelCatalog.models.map(row=>row.model).join('|')==='gpt-6-sol|gpt-6.1-sol-fast','Mapping contains only fetched whitelist matches');
      check((await cc('read_live_provider_settings',{app:'codex'})).config===editedLive.config,'Mode selection does not automatically write tool configuration');
      await waitFor(()=>[...frame.contentDocument.querySelectorAll('[data-key-id="smoke-key"] button')].some(button=>button.textContent.trim()==='应用配置' && !button.disabled),'Pending mode exposes explicit apply');
      [...frame.contentDocument.querySelectorAll('[data-key-id="smoke-key"] button')].find(button=>button.textContent.trim()==='应用配置').click();
      await waitFor(async()=>!(await cc('get_providers',{app:'codex'}))[provider.id].meta.mollyPendingApply,'Mapped apply did not finish');
      const mappedLive = await cc('read_live_provider_settings',{app:'codex'});
      check(mappedLive.config.includes('model_catalog_json') && mappedLive.modelCatalog.models.length===2,'Explicit apply projects the real Codex model catalog');
      check(await invoke('smoke_catalog_matches'),'Disk catalog preserves 1050000 context and all six reasoning levels');
      await cc('switch_provider',{app:'codex',id:'codex-official'});
      check((await cc('get_providers',{app:'codex'}))[provider.id].meta.mollyCodexMode==='mapped','Mapping choice survives switching to Official');
      await cc('switch_provider',{app:'codex',id:provider.id});
      await invoke('set_molly_key_mode',{agent:'codex',keyId:'smoke-key',mode:'native'});
      await cc('switch_provider',{app:'codex',id:provider.id});
      const restored = await cc('read_live_provider_settings',{app:'codex'});
      check(restored.config.includes('gpt-5.5-smoke-edit') && !restored.config.includes('model_catalog_json'),'Native mode restores the edited native model and removes generated mapping');
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
      const minimaxProviders = await cc('get_providers',{app:'mcode'});
      const minimax = Object.values(minimaxProviders).find(item=>item.name==='Molly MiniMax Smoke');
      check(Boolean(minimax), 'MiniMax DB-only import is available through native IPC');
      check(!(await cc('read_live_provider_settings',{app:'mcode'}))[minimax.id], 'MiniMax import preserves live configuration');
      frame.contentWindow.postMessage({source:'mollycloud',type:'navigate',providerId:minimax.id,app:'mcode'},location.origin);
      await waitFor(()=>frame.contentDocument.querySelector('[data-provider-id="'+minimax.id+'"]'), 'MiniMax provider card did not render');
      const minimaxCard = frame.contentDocument.querySelector('[data-provider-id="'+minimax.id+'"]');
      const add = [...minimaxCard.querySelectorAll('button')].find(button=>button.textContent.trim()==='选择并添加');
      check(Boolean(add), 'MiniMax card exposes explicit additive activation');
      add.click();
      await waitFor(async()=>Boolean((await cc('read_live_provider_settings',{app:'mcode'}))[minimax.id]), 'MiniMax activation did not finish');
      check(!(await cc('get_current_provider',{app:'mcode'})), 'MiniMax keeps native default-model selection');
      await invoke('sync_molly_key_providers',{agent:'opencode'});
      check(Object.keys(await cc('get_providers',{app:'opencode'})).length===0,'OpenCode sync adds neither Official nor account providers');
      frame.contentWindow.postMessage({source:'mollycloud',type:'navigate',app:'opencode'},location.origin);
      await waitFor(()=>Boolean(frame.contentDocument.querySelector('.molly-key-record:not([data-provider-id])')),'OpenCode manual directory rendered');
      const choose=[...frame.contentDocument.querySelectorAll('.molly-key-record button')].find(b=>b.textContent==='选择并添加');
      check(Boolean(choose),'OpenCode exposes manual model selection');
      choose.click();
      await waitFor(()=>manualOpenCodeRequest,'OpenCode did not request host model selection');
      check(Object.keys(await cc('get_providers',{app:'opencode'})).length===0,'Selecting OpenCode key waits for model confirmation');
      const openCodeId=await invoke('prepare_smoke_opencode');
      frame.contentWindow.postMessage({source:'mollycloud',type:'apply-provider',app:'opencode',providerId:openCodeId},location.origin);
      await waitFor(async()=>(await cc('get_opencode_live_provider_ids')).includes(openCodeId),'OpenCode confirmed addition did not finish');
      check((await cc('get_opencode_live_provider_ids')).includes('external'),'OpenCode keeps existing provider');
      check(!(await cc('get_current_provider',{app:'opencode'})),'OpenCode does not change default selection');
      frame.contentWindow.__themeProbe = 42;
      for (const theme of ['dark','light']) {
        document.documentElement.dataset.theme = theme;
        await waitFor(()=>frame.contentDocument.documentElement.classList.contains('dark')===(theme==='dark'), 'Embedded theme did not follow '+theme);
        check(frame.contentWindow.__themeProbe===42, 'Theme '+theme+' preserves the iframe');
      }
      frame.contentDocument.querySelector('button[title="设置"]').click();
      await waitFor(()=>[...frame.contentDocument.querySelectorAll('[role="tab"]')].some(tab=>tab.textContent==='关于'), 'About tab did not render');
      const about = [...frame.contentDocument.querySelectorAll('[role="tab"]')].find(tab=>tab.textContent==='关于');
      about.dispatchEvent(new MouseEvent('mousedown',{bubbles:true,button:0}));
      about.click();
      await waitFor(()=>frame.contentDocument.body.textContent.includes('CC Switch 3.20.4 · MollyCloud 内置版'), 'Embedded About version is not 3.20.4');
      check(true, 'About displays embedded CC Switch 3.20.4');
      const external = await cc('parse_deeplink', {url:'ccswitch://v1/import?resource=provider&app=codex&name=External%20Mock&endpoint=https%3A%2F%2Fexample.test%2Fv1&apiKey=mock-not-real'});
      await invoke('plugin:event|emit',{event:'deeplink-import',payload:external});
      await waitFor(()=>Boolean(frame.contentDocument.querySelector('[role="dialog"]')), 'External import confirmation did not render');
      check(!(Object.values(await cc('get_providers',{app:'codex'})).some(item=>item.name==='External Mock')), 'External link never writes before confirmation');
      check(frame.contentDocument.querySelector('[role="dialog"]').textContent.includes('External Mock'), 'External import shows source data for review');
      const secondExternal = await cc('parse_deeplink', {url:'ccswitch://v1/import?resource=provider&app=codex&name=Second%20External&endpoint=https%3A%2F%2Fexample.test%2Fv1&apiKey=mock-second'});
      await invoke('plugin:event|emit',{event:'deeplink-import',payload:secondExternal});
      check(frame.contentDocument.querySelector('[role="dialog"]').textContent.includes('External Mock'), 'Another incoming link does not replace the active confirmation');
      const cancelExternal = [...frame.contentDocument.querySelector('[role="dialog"]').querySelectorAll('button')].find(button=>button.textContent.trim()==='取消');
      check(Boolean(cancelExternal), 'External import can be cancelled');
      cancelExternal.click();
      await waitFor(()=>frame.contentDocument.querySelector('[role="dialog"]')?.textContent.includes('Second External'), 'Queued external import did not appear');
      [...frame.contentDocument.querySelector('[role="dialog"]').querySelectorAll('button')].find(button=>button.textContent.trim()==='取消').click();
      check(!(Object.values(await cc('get_providers',{app:'codex'})).some(item=>item.name==='Second External')), 'Cancelled queued link does not write');
      // Host-managed views reuse original native prompt and MCP CRUD.
      await cc('upsert_prompt',{app:'codex',id:'smoke-prompt',prompt:{id:'smoke-prompt',name:'Isolated Prompt',content:'Synthetic prompt for temporary user only.',enabled:false}});
      frame.contentWindow.postMessage({source:'mollycloud',type:'navigate',app:'codex'},location.origin);
      frame.contentWindow.postMessage({source:'mollycloud',type:'view',view:'prompts'},location.origin);
      await waitFor(()=>frame.contentDocument.querySelector('[data-testid="molly-prompts-view"]')?.textContent.includes('Isolated Prompt'),'Independent prompt view not mounted');
      check(!frame.contentDocument.querySelector('.molly-toolbar button[aria-label="返回"]'),'Standalone prompt view has no API-key back button');
      await cc('enable_prompt',{app:'codex',id:'smoke-prompt'});
      check((await cc('get_current_prompt_file_content',{app:'codex'})).includes('Synthetic prompt'),'Prompt enable writes temporary tool file');
      await cc('upsert_mcp_server',{server:{id:'smoke-mcp',name:'Isolated MCP',server:{command:'synthetic-mcp',args:[]},apps:{}}});
      frame.contentWindow.postMessage({source:'mollycloud',type:'view',view:'mcp'},location.origin);
      await waitFor(()=>frame.contentDocument.querySelector('[data-testid="molly-mcp-view"]')?.textContent.includes('Isolated MCP'),'Independent MCP view not mounted');
      check(!frame.contentDocument.querySelector('.molly-toolbar button[aria-label="返回"]'),'Standalone MCP view has no API-key back button');
      await cc('toggle_mcp_app',{serverId:'smoke-mcp',app:'codex',enabled:true});
      check((await cc('get_mcp_servers'))['smoke-mcp'].apps.codex,'MCP enable persists original native app target');
      frame.contentWindow.postMessage({source:'mollycloud',type:'view',view:'providers'},location.origin);
      await waitFor(()=>frame.contentDocument.querySelector('[data-provider-id]'),'Return to API keys failed');
      check(!frame.contentDocument.querySelector('.molly-toolbar').textContent.includes('CC Switch'),'Provider header removes upstream brand text');
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
            restore_session,
            sync_molly_key_providers,
            set_molly_key_mode,
            smoke_catalog_matches,
            prepare_smoke_opencode
        ])
        .setup(move |app| {
            if !failure_mode {
                std::fs::create_dir_all(check_home.join(".config/opencode"))?;
                std::fs::write(check_home.join(".config/opencode/opencode.json"),r#"{"model":"external/native","provider":{"external":{"npm":"@ai-sdk/openai-compatible","options":{"baseURL":"https://example.test/v1","apiKey":"mock-external"},"models":{"native":{}}}}}"#)?;
                std::fs::create_dir_all(check_home.join(".minimax"))?;
                std::fs::write(check_home.join(".minimax/config.yaml"), "defaultModel: minimax/native\nunknown: preserve-me\n")?;
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
                molly_ccswitch::import_molly_provider(
                    app.handle(),
                    molly_ccswitch::MollyProviderImport {
                        account_id: "smoke-account".into(), key_id: "smoke-key".into(),
                        name: "Molly MiniMax Smoke".into(), app: "mcode".into(),
                        api_key: "sk-molly-smoke-not-a-real-key".into(),
                        base_url: "https://example.test/v1".into(), model: "test-model".into(),
                        usage_script: None,
                    },
                )?;
            }
            tauri::WebviewWindowBuilder::new(
                app,
                "console",
                tauri::WebviewUrl::App("index.html".into()),
            )
            .title("Molly CC Switch isolated verification")
            .visible(cfg!(target_os = "macos"))
            .focused(false)
            .inner_size(1180.0, 760.0)
            .data_directory(webview_data)
            .incognito(cfg!(target_os = "macos"))
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
    // Upstream projects third-party credentials into the provider table
    // in config.toml; auth.json is reserved for an optional native login.
    let live_config =
        std::fs::read_to_string(private_home.join(".codex/config.toml")).unwrap_or_default();
    let private_config_written = !live_config.is_empty();
    let legacy_unchanged = !check_private_home.join(".codex/config.toml").exists();
    let custom_written = custom.join("config.toml").exists();
    let minimax_config = std::fs::read_to_string(private_home.join(".minimax/config.yaml")).unwrap_or_default();
    let minimax_valid = minimax_config.contains("defaultModel: minimax/native")
        && minimax_config.contains("unknown: preserve-me")
        && minimax_config.contains("sk-molly-smoke-not-a-real-key");
    let private_credential_valid =
        molly_ccswitch::extract_codex_experimental_bearer_token(&live_config).as_deref()
            == Some("sk-molly-smoke-not-a-real-key");
    println!(
        "{}",
        serde_json::json!({"native":result,"minimax_valid":minimax_valid,"custom_written":custom_written,"legacy_unchanged":legacy_unchanged,"init_failure_fixture":failure_mode,"external_unchanged":external_unchanged,"system_config_written":private_config_written,"system_credential_valid":private_credential_valid})
    );
    if result["ok"] != true
        || !legacy_unchanged || custom_written == failure_mode
        || !external_unchanged
        || private_config_written == failure_mode
        || private_credential_valid == failure_mode
        || minimax_valid == failure_mode
    {
        std::process::exit(1);
    }
}
