import { useCallback, useEffect, useState } from "react";
import { NavLink, Outlet, useLocation, useNavigate } from "react-router";
import { Images, Tags } from "./icons";
import { TitleBar } from "@/components/TitleBar/TitleBar";
import {
  collectionList,
  savedFiltersList,
  savedFiltersDelete,
  settingsGet,
  settingsSet,
  mediaListTrash,
} from "@/lib/tauri";
import type { SavedFilter } from "@/types/search";
import type { Collection } from "@/types/collection";

function Layout() {
  const [filters, setFilters] = useState<SavedFilter[]>([]);
  const [collections, setCollections] = useState<Collection[]>([]);
  const [trashCount, setTrashCount] = useState(0);
  const [theme, setTheme] = useState<"dark" | "light">("dark");
  const location = useLocation();
  const navigate = useNavigate();

  const loadFilters = useCallback(async () => {
    try {
      setFilters(await savedFiltersList());
    } catch {
      // ignore
    }
  }, []);

  const loadCollections = useCallback(async () => {
    try {
      const all = await collectionList();
      setCollections(all.filter((c) => c.pinned_at));
    } catch {
      // ignore
    }
  }, []);

  const loadTrashCount = useCallback(async () => {
    try {
      const list = await mediaListTrash("imported_at", true);
      setTrashCount(list.length);
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
    loadCollections();
    loadTrashCount();
    loadTheme();
  }, [loadFilters, loadCollections, loadTrashCount, loadTheme, location]);

  // Listen for changes from other pages
  useEffect(() => {
    const handler = () => {
      loadCollections();
      loadTrashCount();
    };
    window.addEventListener("collections-changed", handler);
    return () => window.removeEventListener("collections-changed", handler);
  }, [loadCollections, loadTrashCount]);

  // Listen for saved filter changes from other pages
  useEffect(() => {
    const handler = () => loadFilters();
    window.addEventListener("saved-filters-changed", handler);
    return () => window.removeEventListener("saved-filters-changed", handler);
  }, [loadFilters]);

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
    <div className="flex h-full flex-col bg-[var(--color-bg-primary)] text-[var(--color-text-primary)]">
      <TitleBar />
      <div className="flex flex-1 overflow-hidden">
      {/* Sidebar */}
      <aside className="flex w-60 min-w-[200px] max-w-[320px] flex-col border-r border-[var(--color-border)] bg-[var(--color-bg-secondary)] resize-x overflow-hidden">

        <nav className="flex-1 space-y-1 px-2 py-2 overflow-auto pt-2">
          <NavLink
            to="/media"
            className={({ isActive }) =>
              `flex items-center gap-3 rounded-lg px-3 py-2.5 text-sm font-medium transition-colors ${
                isActive
                  ? "bg-[var(--color-accent-soft)] text-[var(--color-accent)] border-l-[3px] border-[var(--color-accent)]"
                  : "text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)] hover:text-[var(--color-text-primary)] border-l-[3px] border-transparent"
              }`
            }
          >
            <Images className="h-5 w-5" />
            全部媒体
          </NavLink>
          <NavLink
            to="/trash"
            className={({ isActive }) =>
              `flex items-center gap-3 rounded-lg px-3 py-2.5 text-sm font-medium transition-colors ${
                isActive
                  ? "bg-[var(--color-accent-soft)] text-[var(--color-accent)] border-l-[3px] border-[var(--color-accent)]"
                  : "text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)] hover:text-[var(--color-text-primary)] border-l-[3px] border-transparent"
              }`
            }
          >
            <svg className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
              <path strokeLinecap="round" strokeLinejoin="round" d="m14.74 9-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 0 1-2.244 2.077H8.084a2.25 2.25 0 0 1-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 0 0-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 0 1 3.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 0 0-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 0 0-7.5 0" />
            </svg>
            回收站
            {trashCount > 0 && (
              <span className="ml-auto rounded-full bg-[var(--color-danger-soft)] px-1.5 py-px text-[11px] tabular-nums text-[var(--color-danger)]">{trashCount}</span>
            )}
          </NavLink>
          <div className="mt-3 border-t border-[var(--color-border)]" />

          {/* Collections */}
          <>
            <div className="px-3 pt-5 pb-1.5 text-xs font-semibold uppercase tracking-[0.05em] text-[var(--color-text-muted)]">
              集合
            </div>
            {collections.slice(0, 5).map((c) => (
              <NavLink
                key={c.id}
                to={`/collections/${c.id}`}
                className={({ isActive }) =>
                  `flex items-center gap-3 rounded-lg px-3 py-1.5 text-sm font-medium transition-colors ${
                    isActive
                      ? "bg-[var(--color-bg-tertiary)] text-[var(--color-accent)]"
                      : "text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)]"
                  }`
                }
              >
                <svg className="h-4 w-4 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="M2.25 12.75V12A2.25 2.25 0 0 1 4.5 9.75h15A2.25 2.25 0 0 1 21.75 12v.75m-8.69-6.44-2.12-2.12a1.5 1.5 0 0 0-1.061-.44H4.5A2.25 2.25 0 0 0 2.25 6v12a2.25 2.25 0 0 0 2.25 2.25h15A2.25 2.25 0 0 0 21.75 18V9a2.25 2.25 0 0 0-2.25-2.25h-5.379a1.5 1.5 0 0 1-1.06-.44Z" />
                </svg>
                <span className="truncate text-xs">{c.name}</span>
                {c.item_count != null && c.item_count > 0 && (
                  <span className="ml-auto rounded-full bg-[var(--color-bg-tertiary)] px-1.5 py-px text-[11px] tabular-nums text-[var(--color-text-secondary)]">{c.item_count}</span>
                )}
              </NavLink>
            ))}
            <NavLink
              to="/collections"
              className={({ isActive }) =>
                `flex items-center gap-3 rounded-lg px-3 py-1.5 text-xs transition-colors ${
                  isActive
                    ? "bg-[var(--color-bg-tertiary)] text-[var(--color-accent)]"
                    : "text-[var(--color-text-muted)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)]"
                }`
              }
            >
              <svg className="h-4 w-4 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M12 4.5v15m7.5-7.5h-15" />
              </svg>
              全部集合
            </NavLink>
          </>
          <div className="mt-3 border-t border-[var(--color-border)]" />
          <NavLink
            to="/tags"
            className={({ isActive }) =>
              `flex items-center gap-3 rounded-lg px-3 py-2.5 text-sm font-medium transition-colors ${
                isActive
                  ? "bg-[var(--color-accent-soft)] text-[var(--color-accent)] border-l-[3px] border-[var(--color-accent)]"
                  : "text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)] hover:text-[var(--color-text-primary)] border-l-[3px] border-transparent"
              }`
            }
          >
            <Tags className="h-5 w-5" />
            标签
          </NavLink>
          {/* Saved Filters */}
          {filters.length > 0 && (
            <>
              <div className="px-3 pt-5 pb-1.5 text-xs font-semibold uppercase tracking-[0.05em] text-[var(--color-text-muted)]">
                已保存的筛选器
              </div>
              {filters.map((f) => (
                <div key={f.name} className="group relative">
                  <button
                    onClick={() => {
                      const isInCollection = location.pathname.startsWith("/collections/") && location.pathname !== "/collections";
                      const base = isInCollection ? location.pathname : "/media";
                      navigate(`${base}?q=${encodeURIComponent(f.query)}`);
                    }}
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
                    className="absolute right-2 top-1/2 -translate-y-1/2 rounded p-0.5 text-[var(--color-text-muted)] opacity-0 transition-opacity hover:text-[var(--color-danger)] group-hover:opacity-100"
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

        {/* Bottom bar: settings + version + theme */}
        <div className="border-t border-[var(--color-border)] px-2 py-1.5 flex items-center justify-between">
          <div className="flex items-center gap-0.5">
            <NavLink
              to="/settings"
              className={({ isActive }) =>
                `rounded-lg p-1.5 transition-colors ${
                  isActive
                    ? "text-[var(--color-accent)] bg-[var(--color-accent-soft)]"
                    : "text-[var(--color-text-muted)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)]"
                }`
              }
              title="设置"
            >
              <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M9.594 3.94c.09-.542.56-.94 1.11-.94h2.593c.55 0 1.02.398 1.11.94l.213 1.281c.063.374.313.686.645.87.074.04.147.083.22.127.325.196.72.257 1.075.124l1.217-.456a1.125 1.125 0 0 1 1.37.49l1.296 2.247a1.125 1.125 0 0 1-.26 1.431l-1.003.827c-.293.241-.438.613-.43.992a7.723 7.723 0 0 1 0 .255c-.008.378.137.75.43.991l1.004.827c.424.35.534.955.26 1.43l-1.298 2.247a1.125 1.125 0 0 1-1.369.491l-1.217-.456c-.355-.133-.75-.072-1.076.124a6.47 6.47 0 0 1-.22.128c-.331.183-.581.495-.644.869l-.213 1.281c-.09.543-.56.94-1.11.94h-2.594c-.55 0-1.019-.398-1.11-.94l-.213-1.281c-.062-.374-.312-.686-.644-.87a6.52 6.52 0 0 1-.22-.127c-.325-.196-.72-.257-1.076-.124l-1.217.456a1.125 1.125 0 0 1-1.369-.49l-1.297-2.247a1.125 1.125 0 0 1 .26-1.431l1.004-.827c.292-.24.437-.613.43-.991a6.932 6.932 0 0 1 0-.255c.007-.38-.138-.751-.43-.992l-1.004-.827a1.125 1.125 0 0 1-.26-1.43l1.297-2.247a1.125 1.125 0 0 1 1.37-.491l1.216.456c.356.133.751.072 1.076-.124.072-.044.146-.086.22-.128.332-.183.582-.495.644-.869l.214-1.28Z" />
                <path strokeLinecap="round" strokeLinejoin="round" d="M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z" />
              </svg>
            </NavLink>
            <span className="text-[11px] text-[var(--color-text-muted)] select-none">v0.1.0</span>
          </div>
          <button
            onClick={toggleTheme}
            className="rounded-lg p-1.5 text-[var(--color-text-muted)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)] transition-colors"
            title={theme === "dark" ? "切换到浅色模式" : "切换到深色模式"}
          >
            {theme === "dark" ? (
              <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M12 3v2.25m6.364.386-1.591 1.591M21 12h-2.25m-.386 6.364-1.591-1.591M12 18.75V21m-4.773-4.227-1.591 1.591M5.25 12H3m4.227-4.773L5.636 5.636M15.75 12a3.75 3.75 0 1 1-7.5 0 3.75 3.75 0 0 1 7.5 0Z" />
              </svg>
            ) : (
              <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M21.752 15.002A9.72 9.72 0 0 1 18 15.75c-5.385 0-9.75-4.365-9.75-9.75 0-1.33.266-2.597.748-3.752A9.753 9.753 0 0 0 3 11.25C3 16.635 7.365 21 12.75 21a9.753 9.753 0 0 0 9.002-5.998Z" />
              </svg>
            )}
          </button>
        </div>
      </aside>

      {/* Main Content */}
      <main className="flex-1 overflow-auto">
        <Outlet />
      </main>
      </div>
    </div>
  );
}

export default Layout;
