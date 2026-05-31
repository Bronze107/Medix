import { useCallback, useEffect, useState } from "react";
import { useNavigate } from "react-router";
import { convertFileSrc } from "@tauri-apps/api/core";
import { ConfirmDialog } from "@/components/ConfirmDialog/ConfirmDialog";
import type { Collection } from "@/types/collection";
import {
  collectionList,
  collectionCreate,
  collectionDelete,
  collectionRename,
  collectionPin,
  collectionUnpin,
  collectionFirstMediaId,
  mediaThumbnail,
} from "@/lib/tauri";

type SortMode = "name" | "count" | "created";

function CollectionsPage() {
  const [collections, setCollections] = useState<Collection[]>([]);
  const [search, setSearch] = useState("");
  const [sortBy, setSortBy] = useState<SortMode>("created");
  const navigate = useNavigate();

  // Create dialog
  const [showCreate, setShowCreate] = useState(false);
  const [newName, setNewName] = useState("");
  const [newDesc, setNewDesc] = useState("");

  // Rename
  const [renameId, setRenameId] = useState<string | null>(null);
  const [renameText, setRenameText] = useState("");

  // Context menu
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number; col: Collection } | null>(null);

  const [covers, setCovers] = useState<Record<string, string>>({});

  const load = useCallback(async () => {
    try {
      const all = await collectionList();
      setCollections(all);
      // Load covers for collections that have items
      const coverMap: Record<string, string> = {};
      for (const c of all) {
        if ((c.item_count ?? 0) > 0) {
          const mid = await collectionFirstMediaId(c.id);
          if (mid) {
            try {
              const path = await mediaThumbnail(mid);
              coverMap[c.id] = convertFileSrc(path);
            } catch { /* ignore */ }
          }
        }
      }
      setCovers(coverMap);
    } catch (e) {
      console.error("Failed to load collections:", e);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  const filtered = collections
    .filter((c) => c.name.toLowerCase().includes(search.toLowerCase()))
    .sort((a, b) => {
      if (sortBy === "name") return a.name.localeCompare(b.name);
      if (sortBy === "count") return (b.item_count ?? 0) - (a.item_count ?? 0);
      return (b.created_at ?? "").localeCompare(a.created_at ?? "");
    });

  const handleCreate = async () => {
    if (!newName.trim()) return;
    await collectionCreate(newName.trim(), newDesc.trim());
    setShowCreate(false);
    setNewName("");
    setNewDesc("");
    load();
    notify();
  };

  const [deleteId, setDeleteId] = useState<string | null>(null);

  const handleDelete = (id: string) => {
    setDeleteId(id);
  };

  const confirmDelete = async () => {
    if (!deleteId) return;
    await collectionDelete(deleteId);
    setDeleteId(null);
    load();
    notify();
  };

  const handleRename = async (id: string) => {
    if (!renameText.trim()) return;
    await collectionRename(id, renameText.trim());
    setRenameId(null);
    setRenameText("");
    load();
    notify();
  };

  const notify = () => window.dispatchEvent(new CustomEvent("collections-changed"));

  const handlePin = async (col: Collection) => {
    if (col.pinned_at) {
      await collectionUnpin(col.id);
    } else {
      await collectionPin(col.id);
    }
    load();
    notify();
  };

  return (
    <div className="flex h-full flex-col">
      {/* Toolbar */}
      <div className="flex items-center justify-between border-b border-[var(--color-border)] px-6 py-3">
        <h1 className="text-xl font-bold">集合</h1>
        <div className="flex items-center gap-3">
          <input
            type="text"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="搜索集合..."
            className="w-48 rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1 text-xs text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)]"
          />
          <select
            value={sortBy}
            onChange={(e) => setSortBy(e.target.value as SortMode)}
            className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1 text-xs text-[var(--color-text-secondary)] outline-none"
          >
            <option value="created">按创建时间</option>
            <option value="name">按名称</option>
            <option value="count">按数量</option>
          </select>
          <button
            onClick={() => setShowCreate(true)}
            className="rounded bg-[var(--color-accent)] px-3 py-1 text-xs font-medium text-white hover:bg-[var(--color-accent-hover)]"
          >
            新建集合
          </button>
        </div>
      </div>

      {/* Grid */}
      <div className="flex-1 overflow-auto p-6">
        {filtered.length === 0 && (
          <p className="py-20 text-center text-sm text-[var(--color-text-muted)]">
            {search ? "未找到匹配的集合" : "暂无集合，点击\"新建集合\"开始"}
          </p>
        )}
        <div className="grid grid-cols-4 gap-4">
          {filtered.map((c) => (
            <div
              key={c.id}
              data-collection-card
              onClick={() => navigate(`/collections/${c.id}`)}
              onContextMenu={(e) => {
                e.preventDefault();
                setCtxMenu({ x: e.clientX, y: e.clientY, col: c });
              }}
              className="cursor-pointer rounded-lg border border-[var(--color-border)] bg-[var(--color-bg-secondary)] overflow-hidden transition-colors hover:bg-[var(--color-bg-tertiary)]"
            >
              {covers[c.id] ? (
                <img src={covers[c.id]} alt="" className="w-full h-28 object-cover bg-[var(--color-bg-tertiary)]" draggable={false} decoding="async" />
              ) : (
                <div className="w-full h-28 flex items-center justify-center bg-[var(--color-bg-tertiary)]">
                  <svg className="h-8 w-8 text-[var(--color-text-muted)]" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1}>
                    <path strokeLinecap="round" strokeLinejoin="round" d="M2.25 15.75l5.159-5.159a2.25 2.25 0 013.182 0l5.159 5.159m-1.5-1.5l1.409-1.409a2.25 2.25 0 013.182 0l2.909 2.909M3.75 21h16.5A2.25 2.25 0 0022.5 18.75V5.25A2.25 2.25 0 0018 3H5.25A2.25 2.25 0 003 5.25v13.5A2.25 2.25 0 005.25 21z" />
                  </svg>
                </div>
              )}
              <div className="p-4">
                <div className="mb-2 flex items-center gap-2">
                  {c.pinned_at && (
                    <span className="text-[10px] text-yellow-500" title="已置顶">📌</span>
                  )}
                  <h3 className="truncate text-sm font-medium text-[var(--color-text-primary)]">{c.name}</h3>
                </div>
                {c.description && (
                  <p className="mb-2 text-xs text-[var(--color-text-muted)] line-clamp-2">{c.description}</p>
                )}
                <p className="text-[10px] text-[var(--color-text-muted)]">{c.item_count ?? 0} 张</p>
              </div>
            </div>
          ))}
        </div>
      </div>

      {/* Create dialog */}
      {showCreate && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
          onClick={() => setShowCreate(false)}
        >
          <div
            className="w-80 rounded-lg border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] p-4 shadow-xl"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 className="mb-3 text-sm font-bold text-[var(--color-text-primary)]">新建集合</h3>
            <input
              type="text"
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              placeholder="集合名称"
              autoFocus
              onKeyDown={(e) => { if (e.key === "Enter") handleCreate(); }}
              className="mb-2 w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)]"
            />
            <textarea
              value={newDesc}
              onChange={(e) => setNewDesc(e.target.value)}
              placeholder="描述（可选）"
              rows={2}
              onKeyDown={(e) => {
                if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) handleCreate();
              }}
              className="mb-3 w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)]"
            />
            <div className="flex justify-end gap-2">
              <button
                onClick={() => { setShowCreate(false); setNewName(""); setNewDesc(""); }}
                className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] px-3 py-1.5 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
              >
                取消
              </button>
              <button
                onClick={handleCreate}
                disabled={!newName.trim()}
                className="rounded bg-[var(--color-accent)] px-3 py-1.5 text-xs font-medium text-white hover:bg-[var(--color-accent-hover)] disabled:opacity-50"
              >
                创建
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Rename dialog */}
      {renameId && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
          onClick={() => setRenameId(null)}
        >
          <div
            className="w-72 rounded-lg border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] p-4 shadow-xl"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 className="mb-3 text-sm font-bold text-[var(--color-text-primary)]">重命名集合</h3>
            <input
              type="text"
              value={renameText}
              onChange={(e) => setRenameText(e.target.value)}
              placeholder="新名称"
              autoFocus
              onKeyDown={(e) => { if (e.key === "Enter") handleRename(renameId); }}
              className="mb-3 w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)]"
            />
            <div className="flex justify-end gap-2">
              <button
                onClick={() => setRenameId(null)}
                className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] px-3 py-1.5 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
              >
                取消
              </button>
              <button
                onClick={() => handleRename(renameId)}
                disabled={!renameText.trim()}
                className="rounded bg-[var(--color-accent)] px-3 py-1.5 text-xs font-medium text-white hover:bg-[var(--color-accent-hover)] disabled:opacity-50"
              >
                确认
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Context menu */}
      {ctxMenu && (
        <>
          <div
            className="fixed inset-0 z-[59]"
            onClick={() => setCtxMenu(null)}
            onContextMenu={(e) => { e.preventDefault(); setCtxMenu(null); }}
          />
          <div
            className="fixed z-[60] min-w-[130px] rounded-lg border border-[var(--color-border)] bg-[var(--color-bg-tertiary)] py-1 shadow-xl"
            style={{ left: ctxMenu.x, top: ctxMenu.y }}
          >
            <button
              onClick={() => {
                navigate(`/collections/${ctxMenu.col.id}`);
                setCtxMenu(null);
              }}
              className="block w-full px-3 py-1.5 text-left text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
            >
              打开
            </button>
            <button
              onClick={() => {
                setRenameId(ctxMenu.col.id);
                setRenameText(ctxMenu.col.name);
                setCtxMenu(null);
              }}
              className="block w-full px-3 py-1.5 text-left text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
            >
              重命名
            </button>
            <button
              onClick={() => {
                handlePin(ctxMenu.col);
                setCtxMenu(null);
              }}
              className="block w-full px-3 py-1.5 text-left text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
            >
              {ctxMenu.col.pinned_at ? "取消置顶" : "置顶"}
            </button>
            <div className="my-1 border-t border-[var(--color-border)]" />
            <button
              onClick={() => {
                handleDelete(ctxMenu.col.id);
                setCtxMenu(null);
              }}
              className="block w-full px-3 py-1.5 text-left text-xs text-red-400 hover:bg-red-900/20"
            >
              删除
            </button>
          </div>
        </>
      )}
      <ConfirmDialog
        open={deleteId !== null}
        title="删除集合"
        message="确定要删除这个集合吗？图片不会被删除。"
        variant="danger"
        onConfirm={confirmDelete}
        onCancel={() => setDeleteId(null)}
      />
    </div>
  );
}

export default CollectionsPage;
