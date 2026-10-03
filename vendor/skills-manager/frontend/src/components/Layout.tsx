import { useEffect, useLayoutEffect, useRef, type MouseEvent } from "react";
import { createSlidingSelection, animateNavPanel, cancelNavPanel } from "../../../../../app/src/navigationMotion";
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
  const nav = useRef<HTMLElement>(null);
  const content = useRef<HTMLDivElement>(null);
  const motion = useRef<ReturnType<typeof createSlidingSelection> | null>(null);
  const navigationRequest = useRef(0);
  useLayoutEffect(() => {
    if (!nav.current) return;
    motion.current = createSlidingSelection(nav.current, true);
    return () => { motion.current?.dispose(); motion.current = null; };
  }, []);
  useLayoutEffect(() => {
    ++navigationRequest.current;
    motion.current?.update(true);
    const element = content.current;
    if (!element) return;
    element.scrollTop = 0;
    const animation = animateNavPanel(element, true);
    void animation?.finished.then(() => animation.cancel(), () => {});
    return () => cancelNavPanel(element);
  }, [location.pathname]);
  useEffect(() => () => { ++navigationRequest.current; }, []);
  async function selectTab(event: MouseEvent<HTMLAnchorElement>, path: string) {
    if (event.button !== 0 || event.metaKey || event.ctrlKey || event.altKey || event.shiftKey) return;
    event.preventDefault();
    const request = ++navigationRequest.current;
    if (path === location.pathname) {
      const animation = content.current && animateNavPanel(content.current, true);
      if (animation) void animation.finished.then(() => animation.cancel(), () => {});
      return;
    }
    const animation = content.current && animateNavPanel(content.current, false);
    if (animation) { try { await animation.finished; } catch { return; } }
    if (request === navigationRequest.current) navigate(path);
  }
  useModalBoundary();
  useEffect(() => { parent.postMessage({source: "molly-skills", type: "ready"}, window.location.origin); }, []);
  const workspace = /\/(global|lobster)-workspace/.test(location.pathname);
  return <div className="molly-skills-shell">
    <nav ref={nav} className="molly-skills-tabs molly-subnav" aria-label="Skill 管理器功能">
      <i className="selection-indicator" aria-hidden="true"/>
      {tabs.map(([path, name]) => <NavLink key={path} to={path} onClick={event => void selectTab(event, path)} end={path === "/"} className={({isActive}) => isActive || (path === "/global-workspace" && location.pathname.startsWith("/lobster-workspace")) ? "active" : ""}>{name}</NavLink>)}
      <button onClick={() => void refreshAppData()} title="刷新技能状态" aria-label="刷新技能状态"><RefreshCw size={17}/></button>
      <button onClick={openHelp} title="使用帮助" aria-label="使用帮助"><CircleHelp size={17}/></button>
    </nav>
    <div ref={content} className="molly-skills-content">
      {isPreview() && <p className="molly-skills-notice">界面预览 · 安装、部署、移除和备份请在 MollyCloud 桌面端使用。</p>}
      {appError && <StatusBanner compact title="技能数据加载失败" description={appError} actionLabel="重试" onAction={refreshAppData} tone="danger"/>}
      {location.pathname === "/my-skills" && <label className="molly-context-select">预设分组<select className="app-input" value={viewedPreset?.id ?? ""} onChange={e => setViewedPresetId(e.target.value)}><option value="" disabled>全部技能 / 尚无预设</option>{presets.map(p => <option value={p.id} key={p.id}>{p.name} · {p.skill_count}</option>)}</select><NavLink to="/organizer">管理分组</NavLink></label>}
      {workspace && <label className="molly-context-select">查看 Agent<select className="app-input" value={location.pathname} onChange={e => navigate(e.target.value)}><option value="/global-workspace">全部编程 Agent</option><option value="/lobster-workspace">全部个人助手</option>{tools.filter(t => t.installed && t.enabled).map(t => <option key={t.key} value={`/${t.category === "lobster" ? "lobster" : "global"}-workspace/${t.key}`}>{t.display_name}</option>)}</select></label>}
      <Outlet/>
    </div>
    <CommandPalette/>
  </div>;
}
