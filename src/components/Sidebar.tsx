import { useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { NavLink } from "react-router-dom";
import {
  Database,
  FileSearch,
  FileText,
  HelpCircle,
  LayoutDashboard,
  ListChecks,
  PanelLeftClose,
  PanelLeftOpen,
  Settings as SettingsIcon,
  SlidersHorizontal,
} from "lucide-react";
import { getSetting, setSetting } from "../lib/api";

const DEV = import.meta.env.DEV;

const NAV_ITEMS = [
  { to: "/", label: "Dashboard", icon: LayoutDashboard },
  { to: "/new-analysis", label: "New Analysis", icon: FileSearch },
  { to: "/reports", label: "My Reports", icon: FileText },
  { to: "/screener", label: "Screener", icon: SlidersHorizontal },
  { to: "/watchlists", label: "Watchlists", icon: ListChecks },
] as const;

const SIDEBAR_SETTING = "display.sidebar_collapsed";

export default function Sidebar({
  footer,
}: {
  footer: ReactNode;
}) {
  const [collapsed, setCollapsed] = useState(false);

  useEffect(() => {
    let cancel = false;
    getSetting(SIDEBAR_SETTING)
      .then((v) => {
        if (!cancel && v != null) setCollapsed(v === "true");
      })
      .catch(() => {});
    return () => {
      cancel = true;
    };
  }, []);

  const toggle = () => {
    setCollapsed((c) => {
      const next = !c;
      setSetting(SIDEBAR_SETTING, String(next)).catch(() => {});
      return next;
    });
  };
  const listRef = useRef<HTMLElement | null>(null);

  const onKeyDown = (e: KeyboardEvent) => {
    const list = listRef.current;
    if (!list || list !== e.currentTarget) return;
    const items = Array.from(
      list.querySelectorAll<HTMLAnchorElement>("a[data-nav]"),
    );
    const idx = items.indexOf(document.activeElement as HTMLAnchorElement);
    if (idx === -1) return;
    let next = -1;
    if (e.key === "ArrowDown") next = (idx + 1) % items.length;
    else if (e.key === "ArrowUp") next = (idx - 1 + items.length) % items.length;
    else if (e.key === "Home") next = 0;
    else if (e.key === "End") next = items.length - 1;
    if (next !== -1) {
      e.preventDefault();
      items[next]?.focus();
    }
  };

  const linkCls = ({ isActive }: { isActive: boolean }) =>
    `group flex items-center gap-3 rounded-md px-2.5 py-2 text-sm transition ${
      isActive
        ? "bg-app-surface font-medium text-app-text"
        : "text-app-muted hover:bg-app-surface hover:text-app-text"
    }`;

  return (
    <aside
      className={`flex flex-col border-r border-app-border bg-app-sidebar transition-[width] duration-200 ${
        collapsed ? "w-14" : "w-56"
      }`}
    >
      <div className="flex items-center gap-2 px-3 py-4">
        <div className="grid h-7 w-7 shrink-0 place-items-center rounded-md bg-app-text text-xs font-bold text-app-surface">
          S
        </div>
        {!collapsed && (
          <div className="text-base font-semibold tracking-tight text-app-text">
            Stocksy
          </div>
        )}
      </div>

      <nav
        ref={listRef}
        onKeyDown={onKeyDown}
        className="flex flex-col gap-1 px-2"
        aria-label="Primary"
      >
        {NAV_ITEMS.map(({ to, label, icon: Icon }) => (
          <NavLink
            key={to}
            to={to}
            data-nav
            tabIndex={0}
            title={collapsed ? label : undefined}
            className={linkCls}
          >
            <Icon className="h-4 w-4 shrink-0" strokeWidth={1.75} />
            {!collapsed && <span className="truncate">{label}</span>}
          </NavLink>
        ))}
      </nav>

      <div className="mt-auto flex flex-col gap-1 px-2 pb-2">
        <NavLink
          to="/help"
          data-nav
          title={collapsed ? "Help" : undefined}
          className={linkCls}
        >
          <HelpCircle className="h-4 w-4 shrink-0" strokeWidth={1.75} />
          {!collapsed && <span className="truncate">Help</span>}
        </NavLink>
        <NavLink
          to="/settings"
          data-nav
          title={collapsed ? "Settings" : undefined}
          className={linkCls}
        >
          <SettingsIcon className="h-4 w-4 shrink-0" strokeWidth={1.75} />
          {!collapsed && <span className="truncate">Settings</span>}
        </NavLink>
        {DEV && (
          <NavLink
            to="/db-browser"
            data-nav
            title={collapsed ? "DB Browser" : undefined}
            className={linkCls}
          >
            <Database className="h-4 w-4 shrink-0" strokeWidth={1.75} />
            {!collapsed && <span className="truncate">DB Browser</span>}
          </NavLink>
        )}
        <button
          type="button"
          onClick={toggle}
          title={collapsed ? "Expand sidebar" : "Collapse sidebar"}
          className="flex items-center gap-3 rounded-md px-2.5 py-2 text-sm text-app-muted transition hover:bg-app-surface hover:text-app-text"
        >
          {collapsed ? (
            <PanelLeftOpen className="h-4 w-4 shrink-0" strokeWidth={1.75} />
          ) : (
            <PanelLeftClose className="h-4 w-4 shrink-0" strokeWidth={1.75} />
          )}
          {!collapsed && <span className="truncate">Collapse</span>}
        </button>
      </div>

      {footer && !collapsed && (
        <div className="border-t border-app-border px-3 py-2 text-xs text-app-muted">
          {footer}
        </div>
      )}
    </aside>
  );
}