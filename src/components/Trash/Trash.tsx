import { useCallback, useEffect, useRef, useState } from "react";
import type { Media } from "@/types/media";
import { ConfirmDialog } from "@/components/ConfirmDialog/ConfirmDialog";
import { mediaListTrash, mediaRecover, mediaPermanentDelete, mediaEmptyTrash } from "@/lib/tauri";
import Gallery from "@/components/Gallery/Gallery";

type ConfirmType = "permanent" | "empty" | "batchDelete" | "batchRecover" | null;

function Trash() {
  const [media, setMedia] = useState<Media[]>([]);
  const [selected, setSelected] = useState<Media | null>(null);
  const [confirmType, setConfirmType] = useState<ConfirmType>(null);
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number; media: Media } | null>(null);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [lastSelectedIndex, setLastSelectedIndex] = useState<number | null>(null);

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

  // Keyboard shortcuts
  const selectedIdsRef = useRef(selectedIds);
  selectedIdsRef.current = selectedIds;
  const mediaRef = useRef(media);
  mediaRef.current = media;

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
      if (e.key === "Escape") {
        if (selectedIdsRef.current.size > 0) setSelectedIds(new Set());
        else setSelected(null);
      }
      if (e.key === "a" && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        const m = mediaRef.current;
        if (m.length > 0) setSelectedIds(new Set(m.map((x) => x.id)));
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  // Keep lastSelectedIndex in sync with single-select
  useEffect(() => {
    if (selected) {
      const idx = media.findIndex((m) => m.id === selected.id);
      if (idx >= 0) setLastSelectedIndex(idx);
    }
  }, [selected?.id, media]);

  const handleToggleSelect = (_item: Media, index: number, shiftKey: boolean) => {
    if (shiftKey && lastSelectedIndex !== null) {
      const start = Math.min(lastSelectedIndex, index);
      const end = Math.max(lastSelectedIndex, index);
      const rangeIds = media.slice(start, end + 1).map((m) => m.id);
      setSelectedIds((prev) => {
        const next = new Set(prev);
        for (const id of rangeIds) next.add(id);
        return next;
      });
    } else {
      setSelectedIds((prev) => {
        const next = new Set(prev);
        if (next.has(_item.id)) next.delete(_item.id);
        else next.add(_item.id);
        return next;
      });
      setLastSelectedIndex(index);
    }
  };

  const handleRecover = async (id: string) => {
    try {
      await mediaRecover(id);
      setSelected(null);
      setSelectedIds(new Set());
      loadTrash();
      window.dispatchEvent(new CustomEvent("collections-changed"));
    } catch (e) {
      console.error("Failed to recover:", e);
    }
  };

  const handleBatchRecover = async () => {
    for (const id of selectedIds) {
      try { await mediaRecover(id); } catch (e) { console.error("Failed to recover:", id, e); }
    }
    setSelected(null);
    setSelectedIds(new Set());
    loadTrash();
    window.dispatchEvent(new CustomEvent("collections-changed"));
  };

  const [pendingPermanentId, setPendingPermanentId] = useState<string | null>(null);

  const handlePermanentDelete = (id: string) => {
    setPendingPermanentId(id);
    setConfirmType("permanent");
  };

  const confirmPermanentDelete = async () => {
    if (!pendingPermanentId) return;
    try {
      await mediaPermanentDelete(pendingPermanentId);
      setSelected(null);
      setSelectedIds(new Set());
      loadTrash();
      window.dispatchEvent(new CustomEvent("collections-changed"));
    } catch (e) {
      console.error("Failed to permanently delete:", e);
    } finally {
      setConfirmType(null);
      setPendingPermanentId(null);
    }
  };

  const confirmBatchPermanentDelete = async () => {
    for (const id of selectedIds) {
      try { await mediaPermanentDelete(id); } catch (e) { console.error("Failed to permanently delete:", id, e); }
    }
    setSelected(null);
    setSelectedIds(new Set());
    loadTrash();
    window.dispatchEvent(new CustomEvent("collections-changed"));
    setConfirmType(null);
  };

  const handleEmptyTrash = () => {
    setConfirmType("empty");
  };

  const confirmEmptyTrash = async () => {
    try {
      await mediaEmptyTrash();
      setSelected(null);
      setSelectedIds(new Set());
      loadTrash();
      window.dispatchEvent(new CustomEvent("collections-changed"));
    } catch (e) {
      console.error("Failed to empty trash:", e);
    } finally {
      setConfirmType(null);
    }
  };

  return (
    <div className="relative flex h-full flex-col">
      {/* Toolbar */}
      <div className="flex items-center justify-between border-b border-[var(--color-border)] px-4 py-2.5">
        <h1 className="text-xl font-bold">回收站</h1>
        <div className="flex items-center gap-3">
          {media.length > 0 && (
            <button
              onClick={handleEmptyTrash}
              className="rounded-lg px-3 py-1 text-xs font-medium text-[var(--color-danger)] hover:bg-[var(--color-danger-soft)] transition-colors"
            >
              清空回收站
            </button>
          )}
          <span className="text-xs tabular-nums text-[var(--color-text-muted)]">{media.length} 项</span>
        </div>
      </div>

      {/* Content */}
      <div className="flex flex-1 overflow-hidden">
        <div className="flex flex-1 flex-col overflow-hidden p-4">
          {media.length === 0 ? (
            <div className="flex flex-1 items-center justify-center">
              <div className="flex flex-col items-center">
                <svg className="mb-4 h-10 w-10 text-[var(--color-text-muted)]" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="m14.74 9-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 0 1-2.244 2.077H8.084a2.25 2.25 0 0 1-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 0 0-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 0 1 3.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 0 0-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 0 0-7.5 0" />
                </svg>
                <p className="text-sm text-[var(--color-text-muted)]">回收站为空</p>
              </div>
            </div>
          ) : (
            <Gallery
              media={media}
              selectedId={selected?.id ?? null}
              onSelect={setSelected}
              selectedIds={Array.from(selectedIds)}
              onToggleSelect={(item, index, shiftKey) => handleToggleSelect(item, index, shiftKey)}
              onContextMenu={(e, item) => {
                e.preventDefault();
                setSelected(item);
                setCtxMenu({ x: e.clientX, y: e.clientY, media: item });
              }}
            />
          )}
        </div>
      </div>

      {/* Floating batch bar — always rendered to avoid layout shift */}
      <div className={`absolute bottom-0 left-0 right-0 z-30 flex items-center justify-between border-t border-[var(--color-border)] bg-[var(--color-bg-elevated)] px-5 py-2.5 transition-opacity duration-150 ${selectedIds.size > 0 ? "opacity-100" : "opacity-0 pointer-events-none"}`}>
        <button
          onClick={() => setSelectedIds(new Set())}
          className="text-xs text-[var(--color-text-muted)] hover:text-[var(--color-text-primary)] transition-colors"
        >
          已选 <span className="font-semibold text-[var(--color-accent)]">{selectedIds.size}</span> 项 · 取消
        </button>
        <div className="flex items-center gap-1.5">
          <button
            onClick={() => setConfirmType("batchRecover")}
            className="rounded-lg p-1.5 text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)] transition-colors"
            title="批量恢复"
          >
            <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M9 15 3 9m0 0 6-6M3 9h12a6 6 0 0 1 0 12h-3" />
            </svg>
          </button>
          <button
            onClick={() => setConfirmType("batchDelete")}
            className="rounded-lg p-1.5 text-[var(--color-text-muted)] hover:bg-[var(--color-danger-soft)] hover:text-[var(--color-danger)] transition-colors"
            title="批量永久删除"
          >
            <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
              <path strokeLinecap="round" strokeLinejoin="round" d="m14.74 9-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 0 1-2.244 2.077H8.084a2.25 2.25 0 0 1-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 0 0-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 0 1 3.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 0 0-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 0 0-7.5 0" />
            </svg>
          </button>
        </div>
      </div>

      {/* Context menu */}
      {ctxMenu && (
        <>
          <div
            className="fixed inset-0 z-[59]"
            onClick={() => setCtxMenu(null)}
            onContextMenu={(e) => { e.preventDefault(); setCtxMenu(null); }}
          />
          <div
            className="fixed z-[60] min-w-[150px] rounded-xl border border-[var(--color-border)] bg-[var(--color-bg-elevated)]/95 backdrop-blur-xl py-1.5 shadow-2xl shadow-black/30 animate-scale-in"
            style={{ left: ctxMenu.x, top: ctxMenu.y }}
          >
            {selectedIds.has(ctxMenu.media.id) && selectedIds.size > 1 ? (
              <>
                <div className="px-3 py-1.5 text-[11px] text-[var(--color-text-muted)]">
                  已选中 {selectedIds.size} 张图片
                </div>
                <div className="my-1 border-t border-[var(--color-border)]" />
                <button
                  onClick={() => { setSelectedIds(new Set()); setCtxMenu(null); }}
                  className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-[var(--color-text-secondary)] transition-colors hover:bg-[var(--color-bg-hover)]"
                >
                  取消选中
                </button>
                <button
                  onClick={() => { setCtxMenu(null); setConfirmType("batchRecover"); }}
                  className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-[var(--color-text-secondary)] transition-colors hover:bg-[var(--color-bg-hover)] hover:text-[var(--color-text-primary)]"
                >
                  <svg className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                    <path strokeLinecap="round" strokeLinejoin="round" d="M9 15 3 9m0 0 6-6M3 9h12a6 6 0 0 1 0 12h-3" />
                  </svg>
                  恢复 {selectedIds.size} 张图片
                </button>
                <div className="my-1 border-t border-[var(--color-border)]" />
                <button
                  onClick={() => { setCtxMenu(null); setConfirmType("batchDelete"); }}
                  className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-[var(--color-danger)] transition-colors hover:bg-[var(--color-danger-soft)]"
                >
                  <svg className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                    <path strokeLinecap="round" strokeLinejoin="round" d="m14.74 9-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 0 1-2.244 2.077H8.084a2.25 2.25 0 0 1-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 0 0-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 0 1 3.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 0 0-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 0 0-7.5 0" />
                  </svg>
                  永久删除 {selectedIds.size} 张图片
                </button>
              </>
            ) : (
              <>
                <button
                  onClick={() => { handleRecover(ctxMenu.media.id); setCtxMenu(null); }}
                  className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-[var(--color-text-secondary)] transition-colors hover:bg-[var(--color-bg-hover)] hover:text-[var(--color-text-primary)]"
                >
                  <svg className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                    <path strokeLinecap="round" strokeLinejoin="round" d="M9 15 3 9m0 0 6-6M3 9h12a6 6 0 0 1 0 12h-3" />
                  </svg>
                  恢复
                </button>
                <div className="my-1 border-t border-[var(--color-border)]" />
                <button
                  onClick={() => { setCtxMenu(null); handlePermanentDelete(ctxMenu.media.id); }}
                  className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-[var(--color-danger)] transition-colors hover:bg-[var(--color-danger-soft)]"
                >
                  <svg className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                    <path strokeLinecap="round" strokeLinejoin="round" d="m14.74 9-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 0 1-2.244 2.077H8.084a2.25 2.25 0 0 1-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 0 0-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 0 1 3.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 0 0-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 0 0-7.5 0" />
                  </svg>
                  永久删除
                </button>
              </>
            )}
          </div>
        </>
      )}

      <ConfirmDialog
        open={confirmType === "permanent"}
        title="永久删除"
        message="确定要永久删除吗？此操作不可撤销。"
        variant="danger"
        confirmLabel="永久删除"
        onConfirm={confirmPermanentDelete}
        onCancel={() => { setConfirmType(null); setPendingPermanentId(null); }}
      />
      <ConfirmDialog
        open={confirmType === "empty"}
        title="清空回收站"
        message="确定要清空回收站吗？所有已删除的图片将被永久删除，此操作不可撤销。"
        variant="danger"
        confirmLabel="全部清空"
        onConfirm={confirmEmptyTrash}
        onCancel={() => setConfirmType(null)}
      />
      <ConfirmDialog
        open={confirmType === "batchRecover"}
        title="批量恢复"
        message={`确定要恢复选中的 ${selectedIds.size} 张图片吗？`}
        onConfirm={() => { handleBatchRecover(); setConfirmType(null); }}
        onCancel={() => setConfirmType(null)}
      />
      <ConfirmDialog
        open={confirmType === "batchDelete"}
        title="批量永久删除"
        message={`确定要永久删除选中的 ${selectedIds.size} 张图片吗？此操作不可撤销。`}
        variant="danger"
        confirmLabel="永久删除"
        onConfirm={confirmBatchPermanentDelete}
        onCancel={() => setConfirmType(null)}
      />
    </div>
  );
}

export default Trash;
