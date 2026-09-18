//! Hidden WebView2 + real native MCP IPC, isolated Agent homes and mock credentials.
//! cargo run --example mcp_market_smoke --features mcp-market-smoke
use mollycloud_lib::mcp_market::*;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct ResultState(Arc<Mutex<Option<Value>>>);
#[tauri::command]
fn smoke_report(app: tauri::AppHandle, state: tauri::State<'_, ResultState>, result: Value) {
    *state.0.lock().unwrap() = Some(result);
    app.exit(0);
}

#[tauri::command]
fn bootstrap_public() -> Value {
    json!({"service_origin":"https://example.test","health":{"status":"ok"},"settings":{}})
}
#[tauri::command]
fn restore_session() -> Value {
    json!({"id":"smoke","username":"Isolated MCP test","balance":0})
}
#[tauri::command]
fn fetch_dashboard() -> Value {
    json!({"user":restore_session(),"keys":{"items":[]},"usage":{},"subscriptions":{},"subscription_progress":[]})
}
#[tauri::command]
fn get_assistant_config() -> Value {
    json!({"enabled":false,"provider":"mollycloud","model":"test","persona":"","custom_base_url":"","greet_interval":20,"molly_key_id":"","api_key_configured":false})
}
#[tauri::command]
fn is_pet_visible() -> bool {
    false
}
#[tauri::command]
fn set_console_dashboard_active() {}
#[tauri::command]
fn check_desktop_update() -> Option<Value> {
    None
}
#[tauri::command]
fn fetch_account_balance() -> Value {
    json!({"balance":0})
}

const SCRIPT: &str = r#"
(() => {
  if (window.parent !== window) return;
  const native = window.__TAURI_INTERNALS__.invoke;
  const invoke=(cmd,args)=>native(cmd,args);
  const checks=[];
  const check=(condition,name)=>{if(!condition)throw new Error(name);checks.push(name);};
  const wait=async(fn,name,attempts=200)=>{for(let i=0;i<attempts;i++){if(await fn())return;await new Promise(r=>setTimeout(r,50));}throw new Error(name);};
  const set=(el,value)=>{el.value=value;el.dispatchEvent(new Event('input',{bubbles:true}));};
  const button=(scope,text)=>[...scope.querySelectorAll('button')].find(b=>b.textContent.trim()===text);
  let finished=false;
  const report=result=>{if(finished)return;finished=true;invoke('smoke_report',{result:{...result,checks}});};
  setTimeout(()=>report({ok:false,error:'Native smoke timeout'}),240000);
  const run=async()=>{
    try {
      await wait(()=>document.querySelector('[aria-label="MCP 市场"]'),'Marketplace nav missing');
      document.querySelector('button[aria-label="MCP 市场"]').click();
      await wait(()=>document.querySelector('#mcp-experimental-title') || document.querySelector('.mcp-market'),'MCP warning missing');
      if(document.querySelector('#mcp-experimental-title')) button(document.querySelector('.console-settings-dialog'),'我已知晓').click();
      await wait(()=>document.querySelectorAll('.mcp-card').length===21,'Install catalog missing');
      await wait(()=>!button(document.querySelector('.mcp-market'),'手动添加').disabled,'Native status not loaded');
      check(!document.querySelector('.mcp-market').textContent.includes('目录预览'),'Native UI does not pretend to be preview');
      button(document.querySelector('.mcp-market'),'手动添加').click();
      await wait(()=>document.querySelector('[aria-label="MCP 名称"]'),'Install dialog missing');
      set(document.querySelector('[aria-label="MCP 名称"]'),'smoke-custom');
      set(document.querySelector('[aria-label="MCP 连接配置"]'),JSON.stringify(window.__mockSpec));
      for(const el of document.querySelectorAll('.mcp-target .n-checkbox'))if(el.getAttribute('aria-checked')!=='true')el.click();
      button(document.querySelector('.mcp-install-dialog'),'安装到所选 Agent').click();
      await wait(()=>document.querySelectorAll('.mcp-installed-card').length===8,'Four installations plus existing MCPs did not render');
      let status=await invoke('mcp_market_status');
      check(status.installations.filter(r=>r.managed).length===4,'One UI install writes all four Agents');
      check(!JSON.stringify(status).includes('test-only-secret'),'IPC never returns configuration credentials');
      await wait(()=>!document.querySelector('[aria-label="MCP 连接配置"]'),'Secret form did not close after installation');
      check(true,'Secret form is destroyed after the closing animation');
      for(const row of status.installations.filter(r=>r.managed)) {
        status=await invoke('mcp_market_action',{key:row.key,action:'test'});
        check(status.installations.find(r=>r.key===row.key).check.includes('通过'),row.agent+' initializes mock stdio server');
        status=await invoke('mcp_market_action',{key:row.key,action:'disable'});
        check(!status.installations.find(r=>r.key===row.key).enabled,row.agent+' disabled');
        status=await invoke('mcp_market_action',{key:row.key,action:'enable'});
        check(status.installations.find(r=>r.key===row.key).enabled,row.agent+' enabled');
        await invoke('mcp_market_action',{key:row.key,action:'remove'});
      }
      status=await invoke('mcp_market_install',{request:{id:'sequential-thinking',agents:['codex','claude'],values:{}}});
      check(status.installations.some(r=>r.id==='sequential-thinking'&&r.check.includes('通过')),'Real pinned npm package installs through native IPC');
      await invoke('mcp_market_action',{key:'claude:sequential-thinking',action:'disable'});
      status=await invoke('mcp_market_action',{key:'codex:sequential-thinking',action:'uninstall'});
      check(status.message.includes('其他 Agent'),'Uninstall preserves package referenced by a disabled Agent');
      status=await invoke('mcp_market_action',{key:'claude:sequential-thinking',action:'update'});
      check(status.installations.find(r=>r.id==='sequential-thinking').check.includes('通过'),'Package update initializes before switching configuration');
      status=await invoke('mcp_market_action',{key:'claude:sequential-thinking',action:'uninstall'});
      check(status.message.includes('回收站'),'Final uninstall recycles the unshared package');
      for(const pkg of status.packages) await invoke('mcp_market_cleanup',{id:pkg.id,version:pkg.version});
      status=await invoke('mcp_market_status');
      check(status.installations.length===4&&status.installations.every(r=>!r.managed),'Removal retains all pre-existing MCPs');
      let denied=false;
      try{await invoke('mcp_market_install',{request:{id:'existing',agents:['codex'],values:{},custom:window.__mockSpec}});}catch(e){denied=String(e).includes('已存在');}
      check(denied,'Existing Agent entry is never overwritten');
      const online=await invoke('mcp_market_search',{query:'memory'});
      check(online.length>0&&online.every(row=>row.source==='registry'&&row.recipe===null),'Live official Registry returns discovery metadata only');
      const resolution=await invoke('mcp_market_resolve',{source:'awesome',id:'community-a5d3ddeee83b'});
      check(resolution.plans.length>0 && resolution.plans[0].entry.recipe.version!=='latest','Community README produces pinned source plans');
      const plan=resolution.plans[0];
      let rejected=false;
      try{await invoke('mcp_market_install',{request:{id:'tampered',planId:plan.planId,agents:['codex'],values:{}}});}catch(e){rejected=String(e).includes('失效');}
      check(rejected,'Native plan cannot be reused with a forged identity');
      button(document.querySelector('.mcp-market'),'发现').click();
      await wait(()=>document.querySelector('[aria-label="目录来源"]'),'Catalog source selector missing');
      document.querySelector('[aria-label="目录来源"] .n-base-selection').click();
      await wait(()=>[...document.querySelectorAll('.n-base-select-option')].some(el=>el.textContent.includes('社区目录')),'Source menu missing');
      [...document.querySelectorAll('.n-base-select-option')].find(el=>el.textContent.includes('社区目录')).click();
      set(document.querySelector('[aria-label="搜索 MCP"]'),'jae-jae/fetcher-mcp');
      await wait(()=>document.querySelectorAll('.mcp-card').length===1,'Source entry search');
      button(document.querySelector('.mcp-card'),'从来源安装').click();
      await wait(()=>document.querySelector('.mcp-plan-preview'),'Source plan UI missing',1200);
      check(document.querySelector('.mcp-plan-preview').textContent.includes('fetcher-mcp'),'Source package preview renders');
      for(const el of document.querySelectorAll('.mcp-target .n-checkbox'))if(el.getAttribute('aria-checked')!=='true')el.click();
      button(document.querySelector('.mcp-install-dialog'),'安装到所选 Agent').click();
      await wait(()=>!document.querySelector('.mcp-install-dialog') && document.querySelectorAll('.mcp-installed-card').length===8,'Source install failed',2400);
      status=await invoke('mcp_market_status');
      check(status.installations.filter(r=>r.managed).length===4,'Source plan installs to all four Agent formats');
      const row=status.installations.find(r=>r.managed&&r.agent==='codex');
      const cards=[...document.querySelectorAll('.mcp-installed-card')];
      button(cards.find(card=>card.textContent.includes('Codex')&&card.textContent.includes('fetcher-mcp')),'卸载').click();
      await wait(()=>document.querySelector('.mcp-install-dialog')?.textContent.includes('确认卸载'),'Uninstall dialog missing');
      button(document.querySelector('.mcp-install-dialog'),'确认卸载').click();
      await wait(()=>!document.querySelector('.mcp-install-dialog'),'Uninstall UI did not complete',600);
      status=await invoke('mcp_market_status');
      check(!status.installations.some(r=>r.key===row.key),'Uninstall UI removes selected Agent only');
      for(const remaining of status.installations.filter(r=>r.managed)) status=await invoke('mcp_market_action',{key:remaining.key,action:'uninstall'});
      check(status.message.includes('回收站')&&status.packages.length===0,'Source package uninstalls after its last Agent releases it');
      report({ok:true});
    }catch(e){report({ok:false,error:String(e),text:document.body.innerText.slice(-2000)});}
  };
  if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',run,{once:true});else run();
})();
"#;

fn main() {
    let temporary = tempfile::tempdir().unwrap();
    let system_home = temporary.path().join("agent-home");
    let state_root = temporary.path().join("state");
    let packages = temporary.path().join("packages");
    let mock = temporary.path().join("mock-server.cjs");
    std::fs::write(&mock, r#"require('readline').createInterface({input:process.stdin}).on('line',line=>{const r=JSON.parse(line);if(r.method==='initialize')console.log(JSON.stringify({jsonrpc:'2.0',id:r.id,result:{protocolVersion:'2025-03-26',serverInfo:{name:'mock',version:'1'},capabilities:{}}}));});"#).unwrap();
    let node = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|p| p.join("node.exe"))
        .find(|p| p.is_file())
        .expect("Node required");
    let spec = json!({"command":node,"args":[mock],"env":{"MOCK_TOKEN":"test-only-secret"}});
    let result = ResultState(Arc::new(Mutex::new(None)));
    let output = result.clone();
    let mut context = tauri::generate_context!();
    context.config_mut().identifier = "cn.mollycloud.mcp-smoke".into();
    context.config_mut().app.windows.clear();
    let root = temporary.path().to_path_buf();
    let saved_root = root.clone();
    let script = format!("window.__mockSpec={spec};\n{SCRIPT}");
    let app = tauri::Builder::default()
        .manage(result)
        .manage(MarketState::isolated(state_root, packages.clone()))
        .plugin(tauri_plugin_opener::init())
        .plugin(molly_ccswitch::init_for_test(
            root.join("cc-data"),
            system_home,
        ))
        .invoke_handler(tauri::generate_handler![
            smoke_report,
            bootstrap_public,
            restore_session,
            fetch_dashboard,
            get_assistant_config,
            is_pet_visible,
            set_console_dashboard_active,
            check_desktop_update,
            fetch_account_balance,
            mcp_market_status,
            mcp_market_install,
            mcp_market_action,
            mcp_market_search,
            mcp_market_resolve,
            mcp_market_cleanup
        ])
        .setup(move |app| {
            for agent in ["codex", "claude", "opencode", "gemini"] {
                let path = molly_ccswitch::mcp_market::config_path(agent)?;
                assert!(
                    path.starts_with(&root),
                    "test escaped isolated home: {}",
                    path.display()
                );
                let source = if agent == "codex" {
                    "# keep me\nmodel = 'existing-model'\n"
                } else {
                    "{\n  // keep me\n  \"fixture\": true\n}\n"
                };
                let target =
                    molly_ccswitch::mcp_market::target_spec(agent, &json!({"command":"keep-me"}))?;
                let text =
                    molly_ccswitch::mcp_market::patch(agent, source, "existing", Some(&target))?;
                std::fs::create_dir_all(path.parent().unwrap())?;
                std::fs::write(path, text)?;
            }
            tauri::WebviewWindowBuilder::new(
                app,
                "console",
                tauri::WebviewUrl::App("index.html".into()),
            )
            .title("Molly isolated MCP verification")
            .visible(false)
            .skip_taskbar(true)
            .inner_size(1180., 760.)
            .data_directory(root.join("webview"))
            .initialization_script(&script)
            .build()?;
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(250));
                handle.exit(1);
            });
            Ok(())
        })
        .build(context)
        .expect("smoke app");
    app.run_return(|_, _| {});
    let result = output
        .0
        .lock()
        .unwrap()
        .clone()
        .unwrap_or(json!({"ok":false,"error":"watchdog timeout"}));
    println!("{result}");
    assert_eq!(result["ok"], true);
    for agent in ["codex", "claude", "opencode", "gemini"] {
        let path = molly_ccswitch::mcp_market::config_path(agent).unwrap();
        assert!(path.starts_with(&saved_root));
        let text = std::fs::read_to_string(path).unwrap();
        assert!(text.contains("keep me"));
        let entries = molly_ccswitch::mcp_market::entries(agent, &text).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries.contains_key("existing"));
    }
    assert!(
        packages.join("sequential-thinking/data").exists(),
        "Uninstall should retain the per-server data directory"
    );
    println!(
        "Native IPC, source installation UI, shared-package uninstall, cleanup and four Agent formats: passed"
    );
}
