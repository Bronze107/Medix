import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router";
import type { Tag } from "@/types/tag";
import { ConfirmDialog } from "@/components/ConfirmDialog/ConfirmDialog";
import { tagList, tagCreate, tagDelete, tagRename } from "@/lib/tauri";

type SortField = "name" | "count";

function Tags() {
  const [tags, setTags] = useState<Tag[]>([]);
  const [newTagName, setNewTagName] = useState("");
  const [loading, setLoading] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editName, setEditName] = useState("");
  const [search, setSearch] = useState("");
  const [sortBy, setSortBy] = useState<SortField>(
    () => (sessionStorage.getItem("tags_sort") as SortField) || "name"
  );
  const [sortDesc, setSortDesc] = useState(
    () => sessionStorage.getItem("tags_sort_desc") !== "false"
  );

  useEffect(() => {
    sessionStorage.setItem("tags_sort", sortBy);
  }, [sortBy]);
  useEffect(() => {
    sessionStorage.setItem("tags_sort_desc", String(sortDesc));
  }, [sortDesc]);
  const navigate = useNavigate();

  const loadTags = useCallback(async () => {
    try {
      const list = await tagList();
      setTags(list);
    } catch (e) {
      console.error("Failed to load tags:", e);
    }
  }, []);

  useEffect(() => {
    loadTags();
  }, [loadTags]);

  const handleCreate = async () => {
    const name = newTagName.trim();
    if (!name) return;
    setLoading(true);
    try {
      await tagCreate(name);
      setNewTagName("");
      await loadTags();
    } catch (e) {
      console.error("Failed to create tag:", e);
    } finally {
      setLoading(false);
    }
  };

  const unusedCount = useMemo(
    () => tags.filter((t) => (t.item_count ?? 0) === 0).length,
    [tags],
  );

  const [deleteTarget, setDeleteTarget] = useState<{ id: string; name: string } | null>(null);
  const [showCleanConfirm, setShowCleanConfirm] = useState(false);

  const handleDelete = (id: string, name: string) => {
    setDeleteTarget({ id, name });
  };

  const confirmDelete = async () => {
    if (!deleteTarget) return;
    try {
      await tagDelete(deleteTarget.id);
      await loadTags();
    } catch (e) {
      console.error("Failed to delete tag:", e);
    } finally {
      setDeleteTarget(null);
    }
  };

  const startEdit = (tag: Tag) => {
    setEditingId(tag.id);
    setEditName(tag.name);
  };

  const handleCleanUnused = async () => {
    const unused = tags.filter((t) => (t.item_count ?? 0) === 0);
    for (const tag of unused) {
      try { await tagDelete(tag.id); } catch (e) { console.error(e); }
    }
    await loadTags();
    setShowCleanConfirm(false);
  };

  const cancelEdit = () => {
    setEditingId(null);
    setEditName("");
  };

  const handleRename = async (id: string) => {
    const name = editName.trim();
    if (!name) {
      cancelEdit();
      return;
    }
    try {
      await tagRename(id, name);
      setEditingId(null);
      await loadTags();
    } catch (e) {
      console.error("Failed to rename tag:", e);
    }
  };

  const filtered = useMemo(() => {
    let list = tags.filter((t) => t.name.toLowerCase().includes(search.toLowerCase()));
    list.sort((a, b) => {
      const cmp = sortBy === "count"
        ? (a.item_count ?? 0) - (b.item_count ?? 0)
        : a.name.localeCompare(b.name);
      return sortDesc ? -cmp : cmp;
    });
    return list;
  }, [tags, search, sortBy, sortDesc]);

  return (
    <div className="flex h-full flex-col p-6">
      {/* Toolbar */}
      <div className="mb-6 flex items-center gap-3">
        <div className="relative flex-1">
          <svg className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-[var(--color-text-muted)]" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
            <path strokeLinecap="round" strokeLinejoin="round" d="m21 21-5.197-5.197m0 0A7.5 7.5 0 1 0 5.196 5.196a7.5 7.5 0 0 0 10.607 10.607Z" />
          </svg>
          <input
            type="text"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="搜索标签..."
            className="w-full rounded-lg border border-[var(--color-border)] bg-[var(--color-bg-secondary)] py-2 pl-9 pr-3 text-sm text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)] focus:border-[var(--color-accent)]"
          />
        </div>
        <select
          value={sortBy}
          onChange={(e) => setSortBy(e.target.value as SortField)}
          className="rounded-lg border border-[var(--color-border)] bg-[var(--color-bg-secondary)] px-3 py-2 text-sm text-[var(--color-text-secondary)] outline-none"
        >
          <option value="name">按名称</option>
          <option value="count">按数量</option>
        </select>
        <button
          onClick={() => setSortDesc((d) => !d)}
          className="rounded-lg p-2 text-[var(--color-text-muted)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)] transition-colors"
          title={sortDesc ? "降序" : "升序"}
        >
          {sortDesc ? (
            <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M3 4.5h14.25M3 9h9.75M3 13.5h5.25m5.25-.75L17.25 9m0 0L21 12.75M17.25 9v12" />
            </svg>
          ) : (
            <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M3 4.5h14.25M3 9h9.75M3 13.5h9.75m4.5-4.5v12m0 0-3.75-3.75M17.25 21 21 17.25" />
            </svg>
          )}
        </button>
      </div>

      {/* Create bar */}
      <div className="mb-6 flex items-center gap-3">
        <input
          type="text"
          value={newTagName}
          onChange={(e) => setNewTagName(e.target.value)}
          onKeyDown={(e) => { if (e.key === "Enter") handleCreate(); }}
          placeholder="新建标签..."
          className="flex-1 rounded-lg border border-[var(--color-border)] bg-[var(--color-bg-secondary)] px-3 py-2 text-sm text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)] focus:border-[var(--color-accent)]"
        />
        <button
          onClick={handleCreate}
          disabled={loading || !newTagName.trim()}
          className="rounded-lg bg-[var(--color-accent)] px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-[var(--color-accent-hover)] disabled:opacity-50"
        >
          创建
        </button>
        {unusedCount > 0 && (
          <button
            onClick={() => setShowCleanConfirm(true)}
            className="rounded-lg px-3 py-2 text-sm text-[var(--color-text-muted)] hover:text-[var(--color-danger)] transition-colors"
            title={`${unusedCount} 个标签未使用`}
          >
            清理 ({unusedCount})
          </button>
        )}
      </div>

      {/* Tag cards */}
      <div className="flex-1 overflow-y-auto overflow-x-hidden">
        {filtered.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-16">
            <svg className="mb-3 h-8 w-8 text-[var(--color-text-muted)]" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M9.568 3H5.25A2.25 2.25 0 0 0 3 5.25v4.318c0 .597.237 1.17.659 1.591l9.581 9.581c.699.699 1.78.872 2.607.33a18.095 18.095 0 0 0 5.223-5.223c.542-.827.369-1.908-.33-2.607L11.16 3.66A2.25 2.25 0 0 0 9.568 3Z" />
              <path strokeLinecap="round" strokeLinejoin="round" d="M6 6h.008v.008H6V6Z" />
            </svg>
            <p className="text-sm text-[var(--color-text-muted)]">
              {search ? "未找到匹配标签" : "暂无标签，创建一个吧"}
            </p>
          </div>
        ) : (
          <div className="grid grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-3">
            {filtered.map((tag) => (
              <div
                key={tag.id}
                className="group rounded-xl border border-[var(--color-border)] bg-[var(--color-bg-secondary)] p-4 transition-colors hover:border-[var(--color-text-muted)]"
              >
                {editingId === tag.id ? (
                  <div className="flex flex-col gap-2">
                    <input
                      type="text"
                      value={editName}
                      onChange={(e) => setEditName(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") handleRename(tag.id);
                        if (e.key === "Escape") cancelEdit();
                      }}
                      autoFocus
                      className="rounded border border-[var(--color-border)] bg-[var(--color-bg-tertiary)] px-2 py-1 text-sm text-[var(--color-text-primary)] outline-none focus:border-[var(--color-accent)]"
                    />
                    <div className="flex gap-1">
                      <button
                        onClick={() => handleRename(tag.id)}
                        className="rounded bg-[var(--color-accent)] px-2 py-0.5 text-xs text-white hover:bg-[var(--color-accent-hover)]"
                      >
                        保存
                      </button>
                      <button
                        onClick={cancelEdit}
                        className="rounded border border-[var(--color-border)] px-2 py-0.5 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
                      >
                        取消
                      </button>
                    </div>
                  </div>
                ) : (
                  <>
                    <button
                      onClick={() => startEdit(tag)}
                      className="mb-1 truncate text-sm font-medium text-[var(--color-text-primary)] hover:text-[var(--color-accent)] transition-colors"
                      title="点击编辑名称"
                    >
                      {tag.name}
                    </button>
                    <div className="flex items-center justify-between">
                      {(tag.item_count ?? 0) > 0 ? (
                        <button
                          onClick={() => navigate(`/media?q=${encodeURIComponent(`tag:${tag.name}`)}`)}
                          className="text-xs text-[var(--color-text-muted)] hover:text-[var(--color-accent)] transition-colors tabular-nums"
                          title="查看此标签的图片"
                        >
                          {tag.item_count} 张
                        </button>
                      ) : (
                        <span className="text-xs text-[var(--color-text-muted)]">0 张</span>
                      )}
                      <button
                        onClick={() => handleDelete(tag.id, tag.name)}
                        className="rounded p-1 text-[var(--color-text-muted)] opacity-0 transition-all hover:bg-[var(--color-danger-soft)] hover:text-[var(--color-danger)] group-hover:opacity-100"
                        title="删除标签"
                      >
                        <svg className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                          <path strokeLinecap="round" strokeLinejoin="round" d="m14.74 9-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 0 1-2.244 2.077H8.084a2.25 2.25 0 0 1-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 0 0-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 0 1 3.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 0 0-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 0 0-7.5 0" />
                        </svg>
                      </button>
                    </div>
                  </>
                )}
              </div>
            ))}
          </div>
        )}
      </div>

      <ConfirmDialog
        open={deleteTarget !== null}
        title="删除标签"
        message={deleteTarget ? `确定要删除标签 "${deleteTarget.name}" 吗？关联的图片将不再显示此标签。` : ""}
        variant="danger"
        confirmLabel="删除"
        onConfirm={confirmDelete}
        onCancel={() => setDeleteTarget(null)}
      />
      <ConfirmDialog
        open={showCleanConfirm}
        title="清理无用标签"
        message={`确定要删除 ${unusedCount} 个未使用的标签吗？此操作不可撤销。`}
        variant="danger"
        confirmLabel="全部删除"
        onConfirm={handleCleanUnused}
        onCancel={() => setShowCleanConfirm(false)}
      />
    </div>
  );
}

export default Tags;
