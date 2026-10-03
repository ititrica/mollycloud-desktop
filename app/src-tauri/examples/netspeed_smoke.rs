//! Isolated WebView2 check. --startup also shows the test island and starts its
//! production monitors. User profiles, clipboard text, media, notifications,
//! helper processes, and the user's desktop pet are not accessed.
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use tauri::Manager;
#[tauri::command]
fn smoke_progress(stage: String) { println!("Island startup: {stage}"); }
#[tauri::command]
async fn smoke_hide_console(app: tauri::AppHandle) {
    app.get_webview_window("console").unwrap().hide().unwrap();
}
#[tauri::command]
async fn smoke_island_shown(window: tauri::WebviewWindow) -> bool {
    // The upstream island hides by shrinking to 1x1. Check its client dimensions
    // as well as native visibility so a transparent 1px window is not "shown".
    let visible = window.is_visible().unwrap();
    let size = window.inner_size().unwrap();
    visible && size.width > 2 && size.height > 2
}
struct Reports { windows: Mutex<Vec<String>>, passed: Arc<AtomicBool> }
#[tauri::command]
fn smoke_done(window: tauri::WebviewWindow, app: tauri::AppHandle, state: tauri::State<Reports>, error: Option<String>) {
    if let Some(error) = error { eprintln!("Island smoke failed ({}): {error}", window.label()); std::process::exit(1); }
    let mut done = state.windows.lock().unwrap();
    if !done.iter().any(|label| label == window.label()) { done.push(window.label().into()); }
    println!("Island smoke passed: {}", window.label());
    if done.len() == 4 {
        assert!(!app.get_webview_window("console").unwrap().is_visible().unwrap());
        let island = app.get_webview_window("netspeed-widget").unwrap();
        let size = island.inner_size().unwrap();
        assert!(!island.is_visible().unwrap() || (size.width <= 2 && size.height <= 2));
        let passed=state.passed.clone();
        tauri::async_runtime::spawn(async move {
            match molly_netspeed::smoke_activity_api(app.clone()).await {
                Ok(())=>{ println!("Island activity API: create, update, read, delete and origin restriction passed"); passed.store(true,Ordering::SeqCst);app.exit(0); },
                Err(error)=>{eprintln!("Island activity API failed: {error}");std::process::exit(1);}
            }
        });
    }
}
const INIT: &str = r#"
localStorage.setItem('mollycloud:netspeed:nsd_widget_visible', String(window.__smokeStartup));
for(const key of ['music_ctrl','fps_monitor','custom_display','clipboard','msg_notify','activity_api','taskbar_plugin']) localStorage.setItem('mollycloud:netspeed:nsd_'+key,'false');
window.addEventListener('unhandledrejection',e=>window.__TAURI_INTERNALS__.invoke('smoke_done',{error:String(e.reason)}));
"#;
const CONSOLE: &str = r#"
(async()=>{const call=(name,args={})=>window.__TAURI_INTERNALS__.invoke('plugin:molly-netspeed|'+name,args);
const stats=await call('get_network_stats');if(!Array.isArray(stats)||stats.length!==2||stats.some(v=>v<0))throw Error('invalid stats');
await call('configure_activity_api',{enabled:false});
await call('toggle_taskbar_plugin',{enable:false});await call('toggle_fps_plugin',{enable:false});
let rejected=false;try{await call('start_island_animation',{startWidth:210,startHeight:36,targetWidth:300,targetHeight:36,springStyle:'stiff'});}catch{rejected=true;}if(!rejected)throw Error('console animation not denied');
await call('show_window_no_activate',{label:'console'});
await window.__TAURI_INTERNALS__.invoke('smoke_done',{error:null});
})().catch(error=>window.__TAURI_INTERNALS__.invoke('smoke_done',{error:String(error)}));
"#;
const WIDGET: &str = r#"
(async()=>{
const call=(name,args={})=>window.__TAURI_INTERNALS__.invoke('plugin:molly-netspeed|'+name,args);
const progress=stage=>window.__TAURI_INTERNALS__.invoke('smoke_progress',{stage});
const shown=()=>window.__TAURI_INTERNALS__.invoke('smoke_island_shown');
const waitFor=async(test,label)=>{for(let i=0;i<100;i++){if(await test())return;await new Promise(r=>setTimeout(r,100));}throw Error(label);};
await progress('waiting for mounted island');
for(let i=0;i<100&&!document.querySelector('.island-container');i++)await new Promise(r=>setTimeout(r,100));
if(!document.querySelector('.island-container'))throw Error('island UI not mounted');
await new Promise(r=>setTimeout(r,1500));
if(window.__smokeStartup){
await progress('checking automatic show');
await waitFor(shown,'enabled island did not show');
await new Promise(r=>setTimeout(r,700));
await call('set_window_no_activate',{label:'netspeed-widget',enabled:true});
await call('set_window_no_activate',{label:'netspeed-widget',enabled:false});
await progress('window bounds');
await call('set_window_bounds',{x:40,y:40,width:210,height:36});
await progress('expanding island');
await call('start_island_animation',{startWidth:210,startHeight:36,targetWidth:320,targetHeight:70,springStyle:'stiff'});
await new Promise(r=>setTimeout(r,700));
const size=await window.__TAURI_INTERNALS__.invoke('plugin:window|inner_size');
const scale=await window.__TAURI_INTERNALS__.invoke('plugin:window|scale_factor');
if(Math.abs(size.width-320*scale)>2||Math.abs(size.height-70*scale)>2)throw Error('expansion did not reach target: '+JSON.stringify({size,scale}));
await progress('hiding island');
await window.__TAURI_INTERNALS__.invoke('plugin:event|emit',{event:'molly-netspeed:control-island-visibility',payload:{show:false}});
await waitFor(async()=>!await shown(),'island did not hide');
await progress('reopening island');
await window.__TAURI_INTERNALS__.invoke('plugin:event|emit',{event:'molly-netspeed:control-island-visibility',payload:{show:true}});
await waitFor(shown,'island did not reopen');
await new Promise(r=>setTimeout(r,500));
await call('open_console');
await window.__TAURI_INTERNALS__.invoke('smoke_hide_console');
await window.__TAURI_INTERNALS__.invoke('plugin:event|emit',{event:'molly-netspeed:control-island-visibility',payload:{show:false}});
await waitFor(async()=>!await shown(),'reopened island did not hide');
await progress('startup and animation complete');
}
if(await shown())throw Error('disabled island showed on startup');
await call('is_widget_visible');
let rejected=false;try{await call('start_island_animation',{startWidth:210,startHeight:36,targetWidth:-20,targetHeight:36,springStyle:'stiff'});}catch{rejected=true;}if(!rejected)throw Error('invalid bounds accepted');
await window.__TAURI_INTERNALS__.invoke('plugin:event|emit',{event:'molly-netspeed:control-island-opacity',payload:{opacity:55}});
await window.__TAURI_INTERNALS__.invoke('plugin:event|emit',{event:'molly-netspeed:control-glow',payload:{enabled:true}});
await window.__TAURI_INTERNALS__.invoke('plugin:event|emit',{event:'molly-netspeed:control-msg-mode',payload:{enabled:false}});
await new Promise(r=>setTimeout(r,300));
if(!document.querySelector('.has-music-border'))throw Error('console settings not synchronized');
if(await shown())throw Error('quiet setting woke disabled island');
await window.__TAURI_INTERNALS__.invoke('smoke_done',{error:null});
})().catch(error=>window.__TAURI_INTERNALS__.invoke('smoke_done',{error:String(error)}));
"#;
const OUTSIDER: &str = r#"
(async()=>{let rejected=false;try{await window.__TAURI_INTERNALS__.invoke('plugin:molly-netspeed|get_network_stats');}catch{rejected=true;}if(!rejected)throw Error('unrelated window received island access');await window.__TAURI_INTERNALS__.invoke('smoke_done',{error:null});})().catch(error=>window.__TAURI_INTERNALS__.invoke('smoke_done',{error:String(error)}));
"#;
fn main() {
    let startup = std::env::args().any(|arg| arg == "--startup");
    let temporary = tempfile::tempdir().unwrap();
    let profile = temporary.path().join("webview");
    let passed = Arc::new(AtomicBool::new(false));
    let mut context = tauri::generate_context!();
    context.config_mut().identifier = "cn.mollycloud.netspeed-smoke".into();
    context.config_mut().app.windows.clear();
    tauri::Builder::default().plugin(tauri_plugin_opener::init()).plugin(molly_netspeed::init())
        .manage(Reports{windows:Mutex::new(Vec::new()),passed:passed.clone()})
        .invoke_handler(tauri::generate_handler![smoke_done, smoke_progress, smoke_hide_console, smoke_island_shown])
        .setup(move |app| {
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let began = std::time::Instant::now();
                let heartbeat = Arc::new(Mutex::new(std::time::Instant::now()));
                loop {
                    let beat = heartbeat.clone();
                    let _ = handle.run_on_main_thread(move || *beat.lock().unwrap() = std::time::Instant::now());
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    if heartbeat.lock().unwrap().elapsed().as_secs() > 8 || began.elapsed().as_secs() > 45 {
                        eprintln!("Island smoke timed out: UI heartbeat stalled or test incomplete");
                        std::process::exit(2);
                    }
                }
            });
            for label in ["console","netspeed-widget","main","netspeed-outsider"] {
                let is_widget = label == "netspeed-widget";
                let script = if is_widget {WIDGET} else if label=="console" {CONSOLE} else {OUTSIDER};
                let url = if is_widget {"island.html"} else {"netspeed-test.html"};
                tauri::WebviewWindowBuilder::new(app,label,tauri::WebviewUrl::App(url.into()))
                    .visible(false).skip_taskbar(true).transparent(is_widget).decorations(false)
                    .shadow(false).resizable(false).always_on_top(is_widget)
                    .inner_size(if is_widget {210.0} else {400.0}, if is_widget {36.0} else {200.0})
                    .data_directory(profile.clone()).initialization_script(format!("window.__smokeStartup={startup};\n{INIT}"))
                    .on_web_resource_request(move |request,response| { if !is_widget && request.uri().path().ends_with("netspeed-test.html") { *response.body_mut()=std::borrow::Cow::Owned(b"<!doctype html><body>Isolated island smoke</body>".to_vec()); response.headers_mut().insert("Content-Type","text/html".parse().unwrap()); *response.status_mut()=tauri::http::StatusCode::OK; } })
                    .on_page_load(move |window,payload| {if payload.event()==tauri::webview::PageLoadEvent::Finished {let _=window.eval(script);}})
                    .build()?;
            }
            if startup { molly_netspeed::start(app.handle()); }
            Ok(())
        }).run(context).unwrap();
    assert!(passed.load(Ordering::SeqCst),"Island native smoke did not finish");
}
