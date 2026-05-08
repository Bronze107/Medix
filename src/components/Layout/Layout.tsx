import { NavLink, Outlet } from "react-router";
import { Images, Tags, Settings } from "./icons";

function Layout() {
  return (
    <div className="flex h-full bg-neutral-900 text-neutral-100">
      {/* Sidebar */}
      <aside className="flex w-56 flex-col border-r border-neutral-800 bg-neutral-900">
        <div className="flex items-center gap-2 px-4 py-4">
          <div className="h-8 w-8 rounded bg-blue-600" />
          <span className="text-lg font-bold">Medix</span>
        </div>

        <nav className="flex-1 space-y-1 px-2 py-2">
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
