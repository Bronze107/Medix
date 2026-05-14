import { useCallback, useEffect, useState } from "react";
import { NavLink, Outlet, useNavigate } from "react-router";
import { Images, Tags, Settings } from "./icons";
import { savedFiltersList, savedFiltersDelete } from "@/lib/tauri";
import type { SavedFilter } from "@/types/search";

function Layout() {
  const [filters, setFilters] = useState<SavedFilter[]>([]);
  const navigate = useNavigate();

  const loadFilters = useCallback(async () => {
    try {
      setFilters(await savedFiltersList());
    } catch {
      // ignore
    }
  }, []);

  useEffect(() => {
    loadFilters();
  }, [loadFilters]);

  return (
    <div className="flex h-full bg-neutral-900 text-neutral-100">
      {/* Sidebar */}
      <aside className="flex w-56 flex-col border-r border-neutral-800 bg-neutral-900">
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
                  ? "bg-neutral-800 text-blue-400"
                  : "text-neutral-400 hover:bg-neutral-800 hover:text-neutral-200"
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
                  ? "bg-neutral-800 text-blue-400"
                  : "text-neutral-400 hover:bg-neutral-800 hover:text-neutral-200"
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
                  ? "bg-neutral-800 text-blue-400"
                  : "text-neutral-400 hover:bg-neutral-800 hover:text-neutral-200"
              }`
            }
          >
            <Settings className="h-5 w-5" />
            设置
          </NavLink>

          {/* Saved Filters */}
          {filters.length > 0 && (
            <>
              <div className="px-3 pt-4 pb-1 text-[10px] font-semibold uppercase tracking-wider text-neutral-600">
                已保存的筛选器
              </div>
              {filters.map((f) => (
                <div key={f.name} className="group relative">
                  <button
                    onClick={() => navigate(`/media?q=${encodeURIComponent(f.query)}`)}
                    className="flex w-full items-center gap-3 rounded-lg px-3 py-1.5 text-left text-xs text-neutral-400 transition-colors hover:bg-neutral-800 hover:text-neutral-200"
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
                    className="absolute right-2 top-1/2 -translate-y-1/2 rounded p-0.5 text-neutral-700 opacity-0 transition-opacity hover:text-red-400 group-hover:opacity-100"
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

        <div className="border-t border-neutral-800 px-4 py-3 text-xs text-neutral-500">
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
