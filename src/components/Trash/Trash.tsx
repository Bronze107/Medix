import { useCallback, useEffect, useState } from "react";
import type { Media } from "@/types/media";
import { mediaListTrash, mediaRecover, mediaPermanentDelete, mediaEmptyTrash } from "@/lib/tauri";
import Gallery from "@/components/Gallery/Gallery";

function Trash() {
  const [media, setMedia] = useState<Media[]>([]);
  const [selected, setSelected] = useState<Media | null>(null);

  const loadTrash = useCallback(async () => {
    try {
      const list = await mediaListTrash("imported_at", true);
      setMedia(list);
    } catch (e) {
      console.error("Failed to load trash:", e);
    }
  }, []);

  useEffect(() => {
    loadTrash();
  }, [loadTrash]);

  const handleRecover = async (id: string) => {
    try {
      await mediaRecover(id);
      setSelected(null);
      loadTrash();
    } catch (e) {
      console.error("Failed to recover:", e);
    }
  };

  const handlePermanentDelete = async (id: string) => {
    if (!confirm("确定要永久删除吗？此操作不可撤销。")) return;
    try {
      await mediaPermanentDelete(id);
      setSelected(null);
      loadTrash();
    } catch (e) {
      console.error("Failed to permanently delete:", e);
    }
  };

  const handleEmptyTrash = async () => {
    if (!confirm("确定要清空回收站吗？所有已删除的图片将被永久删除，此操作不可撤销。")) return;
    try {
      await mediaEmptyTrash();
      setSelected(null);
      loadTrash();
    } catch (e) {
      console.error("Failed to empty trash:", e);
    }
  };

  return (
    <div className="flex h-full flex-col">
      {/* Toolbar */}
      <div className="flex items-center justify-between border-b border-[var(--color-border)] px-6 py-3">
        <h1 className="text-xl font-bold">回收站</h1>
        <div className="flex items-center gap-3">
          {media.length > 0 && (
            <button
              onClick={handleEmptyTrash}
              className="rounded border border-red-800/50 bg-red-900/20 px-2 py-1 text-xs text-red-400 hover:bg-red-900/30"
            >
              清空回收站
            </button>
          )}
          <span className="text-xs text-[var(--color-text-muted)]">{media.length} 项</span>
        </div>
      </div>

      {/* Content */}
      <div className="flex flex-1 overflow-hidden">
        <div className="flex flex-1 flex-col p-4">
          {media.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-20">
              <svg className="mb-4 h-10 w-10 text-[var(--color-text-muted)]" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                <path strokeLinecap="round" strokeLinejoin="round" d="m14.74 9-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 0 1-2.244 2.077H8.084a2.25 2.25 0 0 1-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 0 0-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 0 1 3.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 0 0-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 0 0-7.5 0" />
              </svg>
              <p className="text-sm text-[var(--color-text-muted)]">回收站为空</p>
            </div>
          ) : (
            <Gallery
              media={media}
              selectedId={selected?.id ?? null}
              onSelect={setSelected}
              selectedIds={[]}
              selectionMode={false}
              onToggleSelect={() => {}}
            />
          )}
        </div>

        {/* Side panel for selected item */}
        <div className="flex h-full w-72 flex-col border-l border-[var(--color-border)] bg-[var(--color-bg-secondary)] p-4">
          {selected ? (
            <>
              <div className="mb-4">
                <p className="text-xs text-[var(--color-text-muted)]">ID</p>
                <p className="mt-0.5 break-all font-mono text-xs text-[var(--color-text-secondary)]">{selected.id}</p>
                <p className="mt-3 text-xs text-[var(--color-text-muted)]">删除时间</p>
                <p className="mt-0.5 text-xs text-[var(--color-text-secondary)]">{selected.deleted_at ? new Date(selected.deleted_at).toLocaleString("zh-CN") : "—"}</p>
              </div>
              <div className="space-y-2">
                <button
                  onClick={() => handleRecover(selected.id)}
                  className="w-full rounded bg-blue-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-blue-500"
                >
                  恢复
                </button>
                <button
                  onClick={() => handlePermanentDelete(selected.id)}
                  className="w-full rounded border border-red-800/50 bg-red-900/20 px-3 py-1.5 text-xs text-red-400 hover:bg-red-900/30"
                >
                  永久删除
                </button>
              </div>
            </>
          ) : (
            <p className="text-sm text-[var(--color-text-muted)]">选择一张图片查看</p>
          )}
        </div>
      </div>
    </div>
  );
}

export default Trash;
