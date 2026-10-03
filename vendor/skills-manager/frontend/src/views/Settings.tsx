import { useEffect, useState } from "react";
import { ArrowUp, FolderOpen, Plus, Settings2 } from "lucide-react";
import { toast } from "sonner";
import { useApp } from "../context/AppContext";
import * as api from "../lib/tauri";
import { open } from "../embedded/bridge";
import { AgentIcon } from "../components/AgentIcon";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { AgentControlSetupCard } from "../components/AgentControlSetupCard";

type Edit = {tool: api.ToolInfo | null; kind: "global" | "project" | "add"};
export function Settings() {
 const {tools, refreshTools} = useApp();
 const [settings, setSettings] = useState<Record<string,string>>({});
 const [repo, setRepo] = useState("");
 const [repoDraft, setRepoDraft] = useState("");
 const [warnings, setWarnings] = useState<string[]>([]);
 const [busy, setBusy] = useState(false);
 const [error, setError] = useState("");
 const [filter, setFilter] = useState("");
 const [edit, setEdit] = useState<Edit | null>(null);
 const [path, setPath] = useState("");
 const [name, setName] = useState("");
 const [key, setKey] = useState("");
 const [remove, setRemove] = useState<api.ToolInfo | null>(null);
 const [relocate, setRelocate] = useState(false);
 const [order, setOrder] = useState<string[]>([]);
 useEffect(() => { Promise.all([
   ...["sync_mode", "proxy_url", "auto_update_check_interval", "auto_update_apply", "git_backup_engine", "merge_engine"].map(async key => [key, await api.getSettings(key)] as const),
 ]).then(rows => setSettings(Object.fromEntries(rows.map(([k,v]) => [k,v ?? ""])))).catch(e => setError(String(e)));
 api.getCentralRepoPath().then(v => {setRepo(v);setRepoDraft(v);}).catch(e => setError(String(e)));
 api.getCentralRepoWarnings().then(setWarnings).catch(e => setError(String(e)));
 api.getToolOrder().then(setOrder).catch(e => setError(String(e)));
 }, []);
 async function action(fn: () => Promise<unknown>) { setBusy(true);setError("");try {await fn();await refreshTools();}catch(e){setError(String(e));}finally{setBusy(false);} }
 const save = (key: string, value: string) => action(async () => {await api.setSettings(key,value);setSettings(s => ({...s,[key]:value}));toast.success("已保存");});
 const sorted = [...tools].sort((a,b) => {const ai=order.indexOf(a.key),bi=order.indexOf(b.key);return (ai<0?999:ai)-(bi<0?999:bi);});
 const visible = sorted.filter(t => `${t.display_name} ${t.key}`.toLowerCase().includes(filter.toLowerCase()));
 function begin(tool: api.ToolInfo | null, kind: Edit["kind"]) {setEdit({tool,kind});setPath(kind === "project" ? tool?.project_relative_skills_dir ?? "" : tool?.skills_dir ?? "");setName("");setKey("");}
 return <div className="app-page">
   {error && <p className="molly-error" role="alert">{error}</p>}
   {warnings.length > 0 && <p className="molly-error">技能库路径存在启动警告：{warnings.join("、")}。请核对下面的当前目录。</p>}
   <section className="app-panel p-5"><h3 className="molly-section-heading"><FolderOpen size={20}/>技能库与部署</h3>
     <div className="molly-setting-row"><div><strong>部署方式</strong><p>复制兼容性更好；符号链接节省空间，Windows 可能需要开发者模式。</p></div><select className="app-input" disabled={busy} value={settings.sync_mode || "copy"} onChange={e => void save("sync_mode", e.target.value)}><option value="copy">复制</option><option value="symlink">符号链接</option></select></div>
     <div className="molly-setting-row"><div><strong>当前中央技能库</strong><p className="molly-path">{repo}</p></div><button className="app-button-secondary" onClick={() => void action(api.openCentralRepoFolder)}>打开目录</button></div>
     <label className="molly-field">迁移技能库目录<div className="molly-input-row"><input className="app-input" value={repoDraft} onChange={e => setRepoDraft(e.target.value)}/><button className="app-button-secondary" onClick={() => void action(async () => {const chosen = await open({directory:true});if(typeof chosen === "string") setRepoDraft(chosen);})}>浏览</button><button className="app-button-secondary" disabled={busy || !repoDraft.trim() || repoDraft === repo} onClick={() => setRelocate(true)}>保存路径</button></div><span>保存后在下次启动 Molly 时迁移；独立版 Skills Manager 的目录不作为迁移目标。</span></label>
   </section>
   <section className="app-panel p-5"><div className="app-toolbar"><h3 className="molly-section-heading"><Settings2 size={20}/>Agent 目录</h3><button className="app-button-primary" onClick={() => begin(null,"add")}><Plus size={16}/>自定义 Agent</button></div><p className="app-field-description">支持 {tools.filter(t => !t.is_custom).length} 种内置 Agent。检测到目录不代表 CLI 已安装。部署前可修改全局与项目相对路径。</p>
     <div className="app-toolbar my-4"><input className="app-input" aria-label="搜索 Agent" placeholder="搜索 Agent…" value={filter} onChange={e => setFilter(e.target.value)}/><div className="molly-row-actions"><button className="app-button-secondary" disabled={busy} onClick={() => void action(() => api.setAllToolsEnabled(true))}>全部启用</button><button className="app-button-secondary" disabled={busy} onClick={() => void action(() => api.setAllToolsEnabled(false))}>全部停用</button></div></div>
     {visible.map(t => <div className="molly-agent-row" key={t.key}><div className="molly-agent-title"><AgentIcon agentKey={t.key} displayName={t.display_name} className="h-8 w-8"/><strong>{t.display_name}</strong><span className="app-badge">{t.installed ? "已检测到目录" : "未检测到目录"}</span><label><input type="checkbox" checked={t.enabled} disabled={busy} onChange={e => void action(() => api.setToolEnabled(t.key,e.target.checked))}/>启用</label></div><p className="molly-path">{t.skills_dir}</p><div className="molly-row-actions"><button className="app-button-secondary" disabled={busy || sorted[0]?.key === t.key} onClick={() => void action(async () => {const ids=sorted.map(t=>t.key);const i=ids.indexOf(t.key);[ids[i-1],ids[i]]=[ids[i],ids[i-1]];await api.setToolOrder(ids);setOrder(ids);})}><ArrowUp size={15}/>上移</button><button className="app-button-secondary" onClick={() => begin(t,"global")}>全局目录</button><button className="app-button-secondary" onClick={() => begin(t,"project")}>项目路径</button>{t.has_path_override && <button className="app-button-secondary" disabled={busy} onClick={() => void action(() => api.resetCustomToolPath(t.key))}>恢复全局默认</button>}{t.has_project_path_override && <button className="app-button-secondary" disabled={busy} onClick={() => void action(() => api.resetCustomToolProjectPath(t.key))}>恢复项目默认</button>}{t.is_custom && <button className="app-button-secondary" onClick={() => setRemove(t)}>删除 Agent</button>}</div></div>)}
   </section>
   <section className="app-panel p-5"><h3 className="molly-section-heading">网络与更新</h3>
     <label className="molly-field">下载代理<div className="molly-input-row"><input className="app-input" placeholder="留空使用系统环境代理，例如 http://127.0.0.1:7890" value={settings.proxy_url ?? ""} onChange={e => setSettings(s=>({...s,proxy_url:e.target.value}))}/><button className="app-button-secondary" disabled={busy} onClick={() => void save("proxy_url",settings.proxy_url.trim())}>保存</button></div></label>
     <div className="molly-setting-row"><div><strong>技能更新检查</strong><p>只检查有远程来源的技能。</p></div><select className="app-input" disabled={busy} value={settings.auto_update_check_interval || "off"} onChange={e=>void save("auto_update_check_interval",e.target.value)}><option value="off">关闭定时检查</option><option value="1h">每小时</option><option value="6h">每 6 小时</option><option value="24h">每天</option></select></div>
     <div className="molly-setting-row"><div><strong>自动应用技能更新</strong><p>开启后会更新技能库和受管理的部署；发生冲突时仍需要处理。</p></div><input aria-label="自动应用技能更新" type="checkbox" disabled={busy} checked={settings.auto_update_apply === "on"} onChange={e=>void save("auto_update_apply",e.target.checked?"on":"off")}/></div>
     <div className="molly-setting-row"><strong>Git 备份引擎</strong><select className="app-input" disabled={busy} value={settings.git_backup_engine || "system"} onChange={e=>void save("git_backup_engine",e.target.value)}><option value="system">系统 Git</option><option value="git2">内置 Git</option></select></div>
     <div className="molly-setting-row"><strong>合并引擎</strong><select className="app-input" disabled={busy} value={settings.merge_engine || "system"} onChange={e=>void save("merge_engine",e.target.value)}><option value="system">系统 Git</option><option value="object">技能对象合并</option></select></div>
     <button className="app-button-secondary" disabled={busy} onClick={() => void action(async () => {const result=await api.exportLogsZip();toast.success(`日志已导出：${result.zip_path}`);})}>导出诊断日志</button>
   </section>
   <AgentControlSetupCard persistent/>
   <p className="app-field-description">基于 Skills Manager 1.40.0（MIT，Tianliang Zhang）。技能管理数据由 MollyCloud 独立保存。</p>
   {edit && <div className="fixed inset-0 z-50 flex items-center justify-center p-4"><div className="absolute inset-0 bg-black/50 backdrop-blur-sm" onClick={() => !busy && setEdit(null)}/><form className="relative app-panel p-6 w-full max-w-lg flex flex-col gap-4" onSubmit={e=>{e.preventDefault();void action(async()=>{if(edit.kind==="add") await api.addCustomTool(key.trim(),name.trim(),path.trim());else if(edit.tool && edit.kind==="global") await api.setCustomToolPath(edit.tool.key,path.trim());else if(edit.tool) await api.setCustomToolProjectPath(edit.tool.key,path.trim());setEdit(null);});}}><h2 className="app-dialog-title">{edit.kind==="add"?"自定义 Agent":edit.kind==="global"?"全局技能目录":"项目相对路径"}</h2>{edit.kind==="add" && <><input className="app-input" required aria-label="Agent 标识" placeholder="唯一标识，例如 my-agent" pattern="[a-zA-Z0-9_-]+" value={key} onChange={e=>setKey(e.target.value)}/><input className="app-input" required aria-label="Agent 名称" placeholder="显示名称" value={name} onChange={e=>setName(e.target.value)}/></>}<input className="app-input" required aria-label="技能目录" placeholder={edit.kind==="project"?".my-agent/skills":"技能目录的绝对路径"} value={path} onChange={e=>setPath(e.target.value)}/>{edit.kind!=="project" && <button type="button" className="app-button-secondary" onClick={()=>void action(async()=>{const chosen=await open({directory:true});if(typeof chosen==="string")setPath(chosen);})}>浏览文件夹</button>}{error && <p className="molly-error" role="alert">{error}</p>}<div className="molly-row-actions"><button type="button" className="app-button-secondary" disabled={busy} onClick={()=>setEdit(null)}>取消</button><button className="app-button-primary" disabled={busy}>保存</button></div></form></div>}
   <ConfirmDialog open={!!remove} title="删除自定义 Agent" message={`删除“${remove?.display_name}”的管理定义。`} onClose={()=>setRemove(null)} onConfirm={async()=>{if(remove){await api.removeCustomTool(remove.key);await refreshTools();}}}/>
   <ConfirmDialog open={relocate} title="迁移技能库" tone="warning" confirmLabel="保存，下次启动生效" message={`下一次启动 Molly 时，将技能库迁移到 ${repoDraft}。本次会话继续使用当前目录。`} onClose={()=>setRelocate(false)} onConfirm={async()=>{await api.setCentralRepoPath(repoDraft.trim());toast.success("路径已保存，下次启动 Molly 时生效");}}/>
 </div>;
}
