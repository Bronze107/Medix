import { useCallback, useEffect, useState } from "react";
import { NavLink, Outlet, useNavigate } from "react-router";
import { Images, Tags, Settings } from "./icons";
import {
  savedFiltersList,
  savedFiltersDelete,
  settingsGet,
  settingsSet,
} from "@/lib/tauri";
import type { SavedFilter } from "@/types/search";

function Layout() {
  const [filters, setFilters] = useState<SavedFilter[]>([]);
  const [theme, setTheme] = useState<"dark" | "light">("dark");
  const navigate = useNavigate();

  const loadFilters = useCallback(async () => {
    try {
      setFilters(await savedFiltersList());
    } catch {
      // ignore
    }
  }, []);

  const loadTheme = useCallback(async () => {
    try {
      const saved = await settingsGet("theme");
      const t = saved === "light" ? "light" : "dark";
      setTheme(t);
      document.documentElement.classList.toggle("dark", t === "dark");
    } catch {
      // use default dark
    }
  }, []);

  useEffect(() => {
    loadFilters();
    loadTheme();
  }, [loadFilters, loadTheme]);

  const toggleTheme = async () => {
    const next = theme === "dark" ? "light" : "dark";
    setTheme(next);
    document.documentElement.classList.toggle("dark", next === "dark");
    try {
      await settingsSet("theme", next);
    } catch {
      // ignore
    }
  };

  return (
    <div className="flex h-full bg-[var(--color-bg-primary)] text-[var(--color-text-primary)]">
      {/* Sidebar */}
      <aside className="flex w-56 flex-col border-r border-[var(--color-border)] bg-[var(--color-bg-secondary)]">
        <div className="flex items-center gap-2 px-4 py-4">
          <div className="h-8 w-8 rounded bg-blue-600" />
          <span className="text-lg font-bold">Medix</span>
        </div>

        <nav className="flex-1 space-y-1 px-2 py-2 overflow-auto">
          <NavLink
            to="/media"
            className={({ isActive }) =>
              `flex items-center gap-3 rounded-lg px-3 py-2 text-sm font-medium transition-colors ${
                isActive
                  ? "bg-[var(--color-bg-tertiary)] text-blue-400"
                  : "text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)]"
              }`
            }
          >
            <Images className="h-5 w-5" />
            全部媒体
          </NavLink>
          <NavLink
            to="/tags"
            className={({ isActive }) =>
              `flex items-center gap-3 rounded-lg px-3 py-2 text-sm font-medium transition-colors ${
                isActive
                  ? "bg-[var(--color-bg-tertiary)] text-blue-400"
                  : "text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)]"
              }`
            }
          >
            <Tags className="h-5 w-5" />
            标签
          </NavLink>
          <NavLink
            to="/settings"
            className={({ isActive }) =>
              `flex items-center gap-3 rounded-lg px-3 py-2 text-sm font-medium transition-colors ${
                isActive
                  ? "bg-[var(--color-bg-tertiary)] text-blue-400"
                  : "text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)]"
              }`
            }
          >
            <Settings className="h-5 w-5" />
            设置
          </NavLink>

          {/* Saved Filters */}
          {filters.length > 0 && (
            <>
              <div className="px-3 pt-4 pb-1 text-[10px] font-semibold uppercase tracking-wider text-[var(--color-text-muted)]">
                已保存的筛选器
              </div>
              {filters.map((f) => (
                <div key={f.name} className="group relative">
                  <button
                    onClick={() => navigate(`/media?q=${encodeURIComponent(f.query)}`)}
                    className="flex w-full items-center gap-3 rounded-lg px-3 py-1.5 text-left text-xs text-[var(--color-text-secondary)] transition-colors hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)]"
                  >
                    <svg
                      className="h-4 w-4 flex-shrink-0"
                      fill="none"
                      viewBox="0 0 24 24"
                      stroke="currentColor"
                      strokeWidth={2}
                    >
                      <path
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        d="m21 21-5.197-5.197m0 0A7.5 7.5 0 1 0 5.196 5.196a7.5 7.5 0 0 0 10.607 10.607Z"
                      />
                    </svg>
                    <span className="truncate">{f.name}</span>
                  </button>
                  <button
                    onClick={async (e) => {
                      e.stopPropagation();
                      await savedFiltersDelete(f.name);
                      loadFilters();
                    }}
                    className="absolute right-2 top-1/2 -translate-y-1/2 rounded p-0.5 text-[var(--color-text-muted)] opacity-0 transition-opacity hover:text-red-400 group-hover:opacity-100"
                    title="删除筛选器"
                  >
                    <svg className="h-3 w-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                      <path strokeLinecap="round" strokeLinejoin="round" d="M6 18 18 6M6 6l12 12" />
                    </svg>
                  </button>
                </div>
              ))}
            </>
          )}
        </nav>

        {/* Theme toggle */}
        <div className="border-t border-[var(--color-border)] px-3 py-2">
          <button
            onClick={toggleTheme}
            className="flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)] transition-colors"
          >
            {theme === "dark" ? (
              <>
                <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="M12 3v2.25m6.364.386-1.591 1.591M21 12h-2.25m-.386 6.364-1.591-1.591M12 18.75V21m-4.773-4.227-1.591 1.591M5.25 12H3m4.227-4.773L5.636 5.636M15.75 12a3.75 3.75 0 1 1-7.5 0 3.75 3.75 0 0 1 7.5 0Z" />
                </svg>
                浅色模式
              </>
            ) : (
              <>
                <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="M21.752 15.002A9.72 9.72 0 0 1 18 15.75c-5.385 0-9.75-4.365-9.75-9.75 0-1.33.266-2.597.748-3.752A9.753 9.753 0 0 0 3 11.25C3 16.635 7.365 21 12.75 21a9.753 9.753 0 0 0 9.002-5.998Z" />
                </svg>
                深色模式
              </>
            )}
          </button>
        </div>

        <div className="border-t border-[var(--color-border)] px-4 py-3 text-xs text-[var(--color-text-muted)]">
          Medix v0.1.0
        </div>
      </aside>

      {/* Main Content */}
      <main className="flex-1 overflow-auto">
        <Outlet />
      </main>
    </div>
  );
}

export default Layout;
