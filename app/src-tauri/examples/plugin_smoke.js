(() => {
  if (window !== window.top) return;
  addEventListener('DOMContentLoaded', async () => {
    const invoke=(cmd,args)=>window.__TAURI_INTERNALS__.invoke(cmd,args);
    const assert=(test,message)=>{if(!test)throw new Error(message);};
    let frame;
    const probe = expected => new Promise((resolve,reject) => {
      frame?.remove(); frame=document.createElement('iframe');
      const timer=setTimeout(()=>{removeEventListener('message',listen);reject(new Error('Iframe probe timeout: '+expected));},5000);
      function listen(event){
        if(event.source!==frame.contentWindow||!event.data?.probe)return;
        removeEventListener('message',listen);clearTimeout(timer);
        try{assert(event.data.probe===expected && event.data.sameOrigin,'Wrong package or origin');resolve();}catch(e){reject(e);}
      }
      addEventListener('message',listen);
      frame.src='/molly-plugins/ccswitch/index.html?v='+Date.now(); document.body.append(frame);
    });
    try {
      localStorage.setItem('mock-user-data','preserved');
      await probe('new');
      await invoke('change_console_plugin',{id:'ccswitch',action:'rollback'});
      await probe('old');
      const states=await invoke('change_console_plugin',{id:'ccswitch',action:'uninstall'});
      assert(!states.find(p=>p.id==='ccswitch').usable,'Uninstall left plugin enabled');
      assert((await fetch('/molly-plugins/ccswitch/index.html')).status===404,'Uninstall still serves plugin code');
      assert(localStorage.getItem('mock-user-data')==='preserved','User data was removed');
      await invoke('finish',{error:null});
    } catch(error) {await invoke('finish',{error:String(error)});}
  });
})();
