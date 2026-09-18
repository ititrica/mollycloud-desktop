import { useEffect } from "react";
import { NavLink, Outlet, useLocation, useNavigate } from "react-router-dom";
import { RefreshCw, CircleHelp } from "lucide-react";
import { StatusBanner } from "./StatusBanner";
import { CommandPalette } from "./CommandPalette";
import { useApp } from "../context/AppContext";
import { isPreview } from "../embedded/bridge";
import { useModalBoundary } from "../embedded/modal";
const tabs = [["/", "概览"], ["/my-skills", "技能库"], ["/install", "发现与安装"], ["/global-workspace", "Agent 工作区"], ["/organizer", "分组与项目"], ["/backup", "备份同步"], ["/settings", "设置"]];
export function Layout() {
  const { appError, refreshAppData, openHelp, presets, viewedPreset, setViewedPresetId, tools } = useApp();
  const location = useLocation();
  const navigate = useNavigate();
  useModalBoundary();
  useEffect(() => { parent.postMessage({source: "molly-skills", type: "ready"}, window.location.origin); }, []);
  const workspace = /\/(global|lobster)-workspace/.test(location.pathname);
  return <div className="molly-skills-shell">
    <nav className="molly-skills-tabs" aria-label="Skill 管理器功能">
      {tabs.map(([path, name]) => <NavLink key={path} to={path} end={path === "/"} className={({isActive}) => isActive || (path === "/global-workspace" && location.pathname.startsWith("/lobster-workspace")) ? "active" : ""}>{name}</NavLink>)}
      <button onClick={() => void refreshAppData()} title="刷新技能状态" aria-label="刷新技能状态"><RefreshCw size={17}/></button>
      <button onClick={openHelp} title="使用帮助" aria-label="使用帮助"><CircleHelp size={17}/></button>
    </nav>
    <div className="molly-skills-content">
      {isPreview() && <p className="molly-skills-notice">界面预览 · 安装、部署、移除和备份请在 MollyCloud 桌面端使用。</p>}
      {appError && <StatusBanner compact title="技能数据加载失败" description={appError} actionLabel="重试" onAction={refreshAppData} tone="danger"/>}
      {location.pathname === "/my-skills" && <label className="molly-context-select">预设分组<select className="app-input" value={viewedPreset?.id ?? ""} onChange={e => setViewedPresetId(e.target.value)}><option value="" disabled>全部技能 / 尚无预设</option>{presets.map(p => <option value={p.id} key={p.id}>{p.name} · {p.skill_count}</option>)}</select><NavLink to="/organizer">管理分组</NavLink></label>}
      {workspace && <label className="molly-context-select">查看 Agent<select className="app-input" value={location.pathname} onChange={e => navigate(e.target.value)}><option value="/global-workspace">全部编程 Agent</option><option value="/lobster-workspace">全部个人助手</option>{tools.filter(t => t.installed && t.enabled).map(t => <option key={t.key} value={`/${t.category === "lobster" ? "lobster" : "global"}-workspace/${t.key}`}>{t.display_name}</option>)}</select></label>}
      <Outlet/>
    </div>
    <CommandPalette/>
  </div>;
}
