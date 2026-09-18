import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { ArrowUp, Folder, Layers, Plus } from "lucide-react";
import { toast } from "sonner";
import { useApp } from "../context/AppContext";
import * as api from "../lib/tauri";
import { CreatePresetDialog } from "../components/CreatePresetDialog";
import { RenamePresetDialog } from "../components/RenamePresetDialog";
import { AddProjectDialog } from "../components/AddProjectDialog";
import { ConfirmDialog } from "../components/ConfirmDialog";

export function Organizer() {
  const {presets, projects, refreshAppData, refreshProjects, setViewedPresetId} = useApp();
  const navigate = useNavigate();
  const [create, setCreate] = useState(false);
  const [addProject, setAddProject] = useState(false);
  const [rename, setRename] = useState<api.Preset | null>(null);
  const [remove, setRemove] = useState<{id: string; name: string; project: boolean} | null>(null);
  const [busy, setBusy] = useState(false);
  async function action(fn: () => Promise<unknown>) { setBusy(true); try { await fn(); await refreshAppData(); } catch (e) { toast.error(String(e)); } finally { setBusy(false); } }
  const reorder = (index: number, project: boolean) => action(async () => {
    const ids = (project ? projects : presets).map(p => p.id);
    [ids[index-1], ids[index]] = [ids[index], ids[index-1]];
    await (project ? api.reorderProjects(ids) : api.reorderPresets(ids));
  });
  return <div className="app-page">
    <header><h2 className="app-page-title">分组与项目</h2><p className="app-page-subtitle">把常用技能保存为预设，或按项目维护独立的技能工作区。</p></header>
    <section className="app-panel p-5"><div className="app-toolbar mb-4"><h3 className="molly-section-heading"><Layers size={20}/>预设分组</h3><button className="app-button-primary" onClick={() => setCreate(true)}><Plus size={16}/>新建预设</button></div>
      {!presets.length && <p className="molly-empty">还没有预设。在技能库中选择技能，再将它们加入分组。</p>}
      {presets.map((p, i) => <div className="molly-list-row" key={p.id}><button className="molly-list-main" onClick={() => {setViewedPresetId(p.id); navigate("/my-skills");}}><strong>{p.name}</strong><span>{p.skill_count} 个技能{p.description ? ` · ${p.description}` : ""}</span></button><div className="molly-row-actions"><button className="app-button-secondary" disabled={busy || !i} aria-label={`上移 ${p.name}`} onClick={() => void reorder(i,false)}><ArrowUp size={15}/></button><button className="app-button-secondary" onClick={() => setRename(p)}>编辑</button><button className="app-button-secondary" onClick={() => setRemove({...p,project:false})}>删除</button></div></div>)}
    </section>
    <section className="app-panel p-5"><div className="app-toolbar mb-4"><h3 className="molly-section-heading"><Folder size={20}/>项目工作区</h3><button className="app-button-primary" onClick={() => setAddProject(true)}><Plus size={16}/>添加项目</button></div>
      {!projects.length && <p className="molly-empty">添加项目文件夹、扫描项目，或关联任意技能目录。</p>}
      {projects.map((p,i) => <div className="molly-list-row" key={p.id}><button className="molly-list-main" onClick={() => navigate(`/project/${p.id}`)}><strong>{p.name} · {p.skill_count} 个技能</strong><span>{p.path}</span></button><div className="molly-row-actions"><button className="app-button-secondary" disabled={busy || !i} aria-label={`上移 ${p.name}`} onClick={() => void reorder(i,true)}><ArrowUp size={15}/></button><button className="app-button-secondary" onClick={() => setRemove({...p,project:true})}>移除</button></div></div>)}
    </section>
    <CreatePresetDialog open={create} onClose={() => setCreate(false)} onCreate={async (name,description,icon) => {try { await api.createPreset(name,description,icon); await refreshAppData(); } catch(e) {toast.error(String(e)); throw e;}}}/>
    <RenamePresetDialog open={!!rename} currentName={rename?.name ?? ""} currentIcon={rename?.icon} onClose={() => setRename(null)} onRename={async (name,icon) => {if(rename) {await api.updatePreset(rename.id,name,rename.description ?? undefined,icon); await refreshAppData();}}}/>
    <AddProjectDialog open={addProject} onClose={() => setAddProject(false)} onAdded={refreshProjects}/>
    <ConfirmDialog open={!!remove} title={remove?.project ? "移除项目工作区" : "删除预设"} message={remove?.project ? `从管理列表移除“${remove.name}”，项目文件会保留。` : `删除“${remove?.name}”预设，技能库中的技能会保留。`} onClose={() => setRemove(null)} onConfirm={async () => {if(remove) {try {await (remove.project ? api.removeProject(remove.id) : api.deletePreset(remove.id)); await refreshAppData();} catch(e) {toast.error(String(e)); throw e;}}}}/>
  </div>;
}
