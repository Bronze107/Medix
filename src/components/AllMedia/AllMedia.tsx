import { useCallback, useEffect, useRef, useState } from "react";
import { useSearchParams } from "react-router";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { ConfirmDialog } from "@/components/ConfirmDialog/ConfirmDialog";
import { useAppStore } from "@/stores/appStore";
import type { Collection } from "@/types/collection";
import type { Media } from "@/types/media";
import type { Tag } from "@/types/tag";
import {
  captionCreateBatch,
  mediaImport,
  mediaList,
  mediaListByCollection,
  mediaSearch,
  mediaTagAddBatch,
  mediaTagRemoveBatch,
  mediaTagsIntersect,
  savedFiltersSave,
  tagCreate,
  tagList,
} from "@/lib/tauri";
import DropZone from "@/components/DropZone/DropZone";
import Gallery from "@/components/Gallery/Gallery";
import TableView from "@/components/TableView/TableView";
import DetailPanel from "@/components/DetailPanel/DetailPanel";
import SearchBar from "@/components/SearchBar/SearchBar";
import ExportDialog from "@/components/ExportDialog/ExportDialog";
import Lightbox from "@/components/Lightbox/Lightbox";
import { showToast } from "@/components/Toast/Toast";
import { aiPendingCount, collectionAddBatch, collectionGetItemIds, collectionList as loadCollections, collectionRemoveItem as removeFromCollection, mediaFindDuplicates, mediaSoftDelete } from "@/lib/tauri";
import { importZip } from "@/lib/tauri";

type SortField = "imported_at" | "created_at" | "modified_at" | "file_size" | "width" | "height";
type ViewMode = "grid" | "table";
type GroupMode = "none" | "date";

interface GroupInfo {
  label: string;
  startIndex: number;
  count: number;
}

interface DragDropPayload {
  paths: string[];
  position: { x: number; y: number };
}

interface AllMediaProps {
  collectionId?: string;
}

function AllMedia({ collectionId }: AllMediaProps) {
  const [searchParams, setSearchParams] = useSearchParams();
  const initialQuery = searchParams.get("q") ?? "";

  const [media, setMedia] = useState<Media[]>([]);
  const [deleteConfirm, setDeleteConfirm] = useState<"batch" | "single" | null>(null);
  const [pendingDeleteId, setPendingDeleteId] = useState<string | null>(null);
  const [selected, setSelected] = useState<Media | null>(null);
  const detailCollapsed = useAppStore((s) => s.detailCollapsed);
  const setDetailCollapsed = useAppStore((s) => s.setDetailCollapsed);
  const [sortBy, setSortBy] = useState<SortField>("imported_at");
  const [descending, setDescending] = useState(true);
  const [isImporting, setIsImporting] = useState(false);
  const [importMessage, setImportMessage] = useState("");
  const [importProgress, setImportProgress] = useState<{ current: number; total: number; filename: string } | null>(null);
  const [dropHover, setDropHover] = useState(false);
  const [searchQuery, setSearchQuery] = useState(initialQuery);
  const [debouncedSearch, setDebouncedSearch] = useState(initialQuery);
  const [savedFilterName, setSavedFilterName] = useState("");
  const [showSaveDialog, setShowSaveDialog] = useState(false);
  const [showExportDialog, setShowExportDialog] = useState(false);
  const [showImportDialog, setShowImportDialog] = useState(false);
  const [showDupDialog, setShowDupDialog] = useState(false);
  const [dupGroups, setDupGroups] = useState<Media[][]>([]);
  const [findingDups, setFindingDups] = useState(false);
  const [importZipPath, setImportZipPath] = useState("");
  const [importing, setImporting] = useState(false);
  const [importResult, setImportResult] = useState<string | null>(null);
  const [lightboxIndex, setLightboxIndex] = useState<number | null>(null);
  const [aiRemaining, setAiRemaining] = useState(0);
  const [viewMode, setViewMode] = useState<ViewMode>("grid");
  const [groupBy, setGroupBy] = useState<GroupMode>("none");

  // Context menu
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number; media: Media } | null>(null);
  const [showMoreMenu, setShowMoreMenu] = useState(false);

  // Add to collection dialog (shared between context menu and batch)
  const [showAddToCollection, setShowAddToCollection] = useState(false);
  const [addToCollectionMediaIds, setAddToCollectionMediaIds] = useState<string[]>([]);
  const [collectionPickerSearch, setCollectionPickerSearch] = useState("");
  const [collectionsForPicker, setCollectionsForPicker] = useState<Collection[]>([]);

  // Batch selection
  const [selectionMode, setSelectionMode] = useState(false);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());

  // Batch tag dialog
  const [showBatchTagDialog, setShowBatchTagDialog] = useState(false);
  const [batchTagSearch, setBatchTagSearch] = useState("");
  const [allTags, setAllTags] = useState<Tag[]>([]);

  // Batch caption dialog
  const [showBatchCaptionDialog, setShowBatchCaptionDialog] = useState(false);
  const [batchCaptionText, setBatchCaptionText] = useState("");

  // Batch remove tag dialog
  const [showBatchRemoveTagDialog, setShowBatchRemoveTagDialog] = useState(false);
  const [batchRemoveTagSearch, setBatchRemoveTagSearch] = useState("");
  const [intersectTags, setIntersectTags] = useState<Tag[]>([]);

  // Sync URL query param to search state (e.g., when clicking a saved filter)
  const urlQuery = searchParams.get("q") ?? "";
  useEffect(() => {
    setSearchQuery(urlQuery);
    setDebouncedSearch(urlQuery);
  }, [urlQuery]);

  // Debounce search input
  useEffect(() => {
    const timer = setTimeout(() => {
      setDebouncedSearch(searchQuery.trim());
    }, 300);
    return () => clearTimeout(timer);
  }, [searchQuery]);

  const loadSeqRef = useRef(0);

  const loadMedia = useCallback(async () => {
    const seq = ++loadSeqRef.current;
    try {
      const query = debouncedSearch.trim();
      let list: Media[];
      if (collectionId) {
        if (query) {
          const ids = await collectionGetItemIds(collectionId);
          const idSet = new Set(ids);
          const searchResults = await mediaSearch(query, sortBy, descending);
          list = searchResults.filter((m) => idSet.has(m.id));
        } else {
          list = await mediaListByCollection(collectionId, sortBy, descending);
        }
      } else {
        if (query) {
          list = await mediaSearch(query, sortBy, descending);
        } else {
          list = await mediaList(sortBy, descending);
        }
      }
      if (seq !== loadSeqRef.current) return; // Stale response
      setMedia(list);
    } catch (e) {
      console.error("Failed to load media:", e);
    }
  }, [sortBy, descending, debouncedSearch, collectionId]);

  useEffect(() => {
    loadMedia();
  }, [loadMedia]);

  const loadAllTags = useCallback(async () => {
    try {
      const list = await tagList();
      setAllTags(list);
    } catch (e) {
      console.error("Failed to load tags:", e);
    }
  }, []);

  useEffect(() => {
    if (showBatchTagDialog) {
      loadAllTags();
    }
  }, [showBatchTagDialog, loadAllTags]);

  const doImport = useCallback(
    async (paths: string[]) => {
      if (paths.length === 0) return;

      // Split ZIPs from image files
      const zips = paths.filter((p) => p.toLowerCase().endsWith(".zip"));
      const images = paths.filter((p) => !p.toLowerCase().endsWith(".zip"));

      setIsImporting(true);

      try {
        let totalImported = 0;
        let totalDuplicates = 0;
        let totalFailed = 0;
        let newImageIds: string[] = [];

        // Import images
        if (images.length > 0) {
          setImportMessage(`正在导入 ${images.length} 个文件...`);
          const results = await mediaImport(images);
          totalImported += results.filter((r) => r.success && !r.error?.includes("重复")).length;
          totalDuplicates += results.filter((r) => r.success && r.error?.includes("重复")).length;
          totalFailed += results.filter((r) => !r.success).length;
          newImageIds = results
            .filter((r) => r.success && !r.error?.includes("重复"))
            .map((r) => r.id);
        }

        // Import ZIPs
        for (const zipPath of zips) {
          setImportMessage(`正在导入 ZIP: ${zipPath.split("\\").pop() || zipPath}...`);
          try {
            const count = await importZip(zipPath);
            totalImported += count;
          } catch (e) {
            totalFailed++;
            console.error(`ZIP import failed: ${zipPath}`, e);
          }
        }

        const parts: string[] = [];
        if (totalImported > 0) parts.push(`${totalImported} 成功`);
        if (totalDuplicates > 0) parts.push(`${totalDuplicates} 重复`);
        if (totalFailed > 0) parts.push(`${totalFailed} 失败`);
        setImportMessage(`导入完成: ${parts.join(", ")}`);
        await loadMedia();

        // In collection mode, auto-add newly imported images
        if (collectionId && newImageIds.length > 0) {
          await collectionAddBatch(collectionId, newImageIds);
          window.dispatchEvent(new CustomEvent("collections-changed"));
          await loadMedia(); // refresh to show new items in collection
        }
        const pending = await aiPendingCount();
        setAiRemaining(pending);
      } catch (e) {
        setImportMessage(`导入失败: ${e}`);
      } finally {
        setIsImporting(false);
        setDropHover(false);
        setImportProgress(null);
        setTimeout(() => setImportMessage(""), 5000);
      }
    },
    [loadMedia]
  );

  useEffect(() => {
    const unlistenEnter = listen("tauri://drag-enter", () => {
      setDropHover(true);
    });
    const unlistenLeave = listen("tauri://drag-leave", () => {
      setDropHover(false);
    });
    const unlistenDrop = listen<DragDropPayload>("tauri://drag-drop", (event) => {
      setDropHover(false);
      doImport(event.payload.paths);
    });
    const unlistenRemote = listen<string>("remote-import", () => {
      loadMedia();
      setImportMessage("收到来自浏览器的图片，已导入");
      setTimeout(() => setImportMessage(""), 3000);
    });
    const unlistenAiDone = listen<{ remaining: number }>("ai-task-done", (event) => {
      setAiRemaining(event.payload.remaining);
      if (event.payload.remaining === 0) {
        loadMedia(); // auto refresh when all AI tasks complete
      }
    });
    const unlistenImportProgress = listen<{ current: number; total: number; filename: string }>("import-progress", (event) => {
      setImportProgress(event.payload);
    });

    return () => {
      unlistenEnter.then((f) => f());
      unlistenLeave.then((f) => f());
      unlistenDrop.then((f) => f());
      unlistenRemote.then((f) => f());
      unlistenAiDone.then((f) => f());
      unlistenImportProgress.then((f) => f());
    };
  }, [doImport, loadMedia]);

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;

      if (e.key === "Escape") {
        if (selectionMode && selectedIds.size > 0) {
          setSelectedIds(new Set());
        } else {
          setSelectionMode(false);
          setSelectedIds(new Set());
          setSelected(null);
        }
      }
      if (e.key === "a" && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        if (media.length > 0) {
          setSelectionMode(true);
          setSelectedIds(new Set(media.map((m) => m.id)));
        }
      }
      if (e.key === "Delete" && selectionMode && selectedIds.size > 0) {
        e.preventDefault();
        handleBatchDelete();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [selectionMode, selectedIds, media, setSelected, setSelectionMode, setSelectedIds]);

  const [lastSelectedIndex, setLastSelectedIndex] = useState<number | null>(null);

  const handleToggleSelect = (item: Media, index: number, shiftKey: boolean) => {
    if (shiftKey && lastSelectedIndex !== null) {
      // Range select from lastSelectedIndex to index
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
        if (next.has(item.id)) {
          next.delete(item.id);
        } else {
          next.add(item.id);
        }
        return next;
      });
      setLastSelectedIndex(index);
    }
  };

  const handleSelectAll = () => {
    setSelectedIds(new Set(media.map((m) => m.id)));
  };

  const handleBatchTagAdd = async (tagId: string) => {
    if (selectedIds.size === 0) return;
    try {
      await mediaTagAddBatch(Array.from(selectedIds), tagId);
      setShowBatchTagDialog(false);
      setBatchTagSearch("");
      setSelectionMode(false);
      setSelectedIds(new Set());
      setSelected(null);
    } catch (e) {
      console.error("Failed to batch add tag:", e);
    }
  };

  const handleBatchDelete = () => {
    if (selectedIds.size === 0) return;
    setDeleteConfirm("batch");
  };

  const confirmBatchDelete = async () => {
    for (const id of selectedIds) {
      try {
        await mediaSoftDelete(id);
      } catch (e) {
        console.error("Failed to delete:", id, e);
      }
    }
    setSelectedIds(new Set());
    setSelected(null);
    setSelectionMode(false);
    setDeleteConfirm(null);
    loadMedia();
    window.dispatchEvent(new CustomEvent("collections-changed"));
    showToast(`已删除 ${selectedIds.size} 张图片`);
  };

  // Compute date groups from sorted media
  const groups = groupBy === "date"
    ? (() => {
        const result: GroupInfo[] = [];
        let cur = "";
        for (let i = 0; i < media.length; i++) {
          const d = media[i].imported_at?.slice(0, 10) ?? "未知日期";
          if (d !== cur) { cur = d; result.push({ label: d, startIndex: i, count: 0 }); }
          result[result.length - 1].count++;
        }
        return result;
      })()
    : [];

  const handleCreateAndBatchAdd = async () => {
    const name = batchTagSearch.trim().toLowerCase();
    if (!name) return;
    try {
      const existing = allTags.find((t) => t.name.toLowerCase() === name);
      let tagId: string;
      if (existing) {
        tagId = existing.id;
      } else {
        tagId = await tagCreate(name);
      }
      await handleBatchTagAdd(tagId);
    } catch (e) {
      console.error("Failed to create and batch add tag:", e);
    }
  };

  const filteredBatchTags = allTags.filter((t) =>
    t.name.toLowerCase().includes(batchTagSearch.trim().toLowerCase())
  );

  const hasExactMatch = allTags.some(
    (t) => t.name.toLowerCase() === batchTagSearch.trim().toLowerCase()
  );

  return (
    <div className="flex h-full flex-col">
      {/* Toolbar */}
      <div className="relative flex items-center gap-3 border-b border-[var(--color-border)] px-4 py-2.5">
        <SearchBar
          value={searchQuery}
          onChange={setSearchQuery}
          onClear={() => { setSearchQuery(""); setSearchParams({}); }}
        />

        {/* Save filter button */}
        {debouncedSearch && (
          <button
            onClick={() => setShowSaveDialog(true)}
            className="flex-shrink-0 rounded-lg p-1.5 text-[var(--color-text-muted)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)] transition-colors"
            title="保存当前筛选"
          >
            <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M17.593 3.322c1.1.128 1.907 1.077 1.907 2.185V21L12 17.25 4.5 21V5.507c0-1.108.806-2.057 1.907-2.185a48.507 48.507 0 0 1 11.186 0Z" />
            </svg>
          </button>
        )}

        {/* View toggle */}
        <button
          onClick={() => setViewMode((m) => (m === "grid" ? "table" : "grid"))}
          className="flex-shrink-0 rounded-lg p-1.5 text-[var(--color-text-muted)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)] transition-colors"
          title={viewMode === "grid" ? "列表视图" : "网格视图"}
        >
          {viewMode === "grid" ? (
            <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M8.25 6.75h12M8.25 12h12m-12 5.25h12M3.75 6.75h.007v.008H3.75V6.75Zm.375 0a.375.375 0 1 1-.75 0 .375.375 0 0 1 .75 0ZM3.75 12h.007v.008H3.75V12Zm.375 0a.375.375 0 1 1-.75 0 .375.375 0 0 1 .75 0Zm-.375 5.25h.007v.008H3.75v-.008Zm.375 0a.375.375 0 1 1-.75 0 .375.375 0 0 1 .75 0Z" />
            </svg>
          ) : (
            <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M3.75 6A2.25 2.25 0 0 1 6 3.75h2.25A2.25 2.25 0 0 1 10.5 6v2.25a2.25 2.25 0 0 1-2.25 2.25H6a2.25 2.25 0 0 1-2.25-2.25V6ZM3.75 15.75A2.25 2.25 0 0 1 6 13.5h2.25a2.25 2.25 0 0 1 2.25 2.25V18a2.25 2.25 0 0 1-2.25 2.25H6A2.25 2.25 0 0 1 3.75 18v-2.25ZM13.5 6a2.25 2.25 0 0 1 2.25-2.25H18A2.25 2.25 0 0 1 20.25 6v2.25A2.25 2.25 0 0 1 18 10.5h-2.25a2.25 2.25 0 0 1-2.25-2.25V6ZM13.5 15.75a2.25 2.25 0 0 1 2.25-2.25H18a2.25 2.25 0 0 1 2.25 2.25V18A2.25 2.25 0 0 1 18 20.25h-2.25A2.25 2.25 0 0 1 13.5 18v-2.25Z" />
            </svg>
          )}
        </button>

        {/* Sort direction */}
        <button
          onClick={() => setDescending((d) => !d)}
          className="flex-shrink-0 rounded-lg p-1.5 text-[var(--color-text-muted)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)] transition-colors"
          title={descending ? "降序排列" : "升序排列"}
        >
          {descending ? (
            <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M3 4.5h14.25M3 9h9.75M3 13.5h5.25m5.25-.75L17.25 9m0 0L21 12.75M17.25 9v12" />
            </svg>
          ) : (
            <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M3 4.5h14.25M3 9h9.75M3 13.5h9.75m4.5-4.5v12m0 0-3.75-3.75M17.25 21 21 17.25" />
            </svg>
          )}
        </button>

        {/* Selection mode */}
        <button
          onClick={() => {
            setSelectionMode((m) => !m);
            setSelectedIds(new Set());
            if (selectionMode) setSelected(null);
          }}
          className={`flex-shrink-0 rounded-lg p-1.5 transition-colors ${
            selectionMode
              ? "bg-[var(--color-success-soft)] text-[var(--color-success)]"
              : "text-[var(--color-text-muted)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)]"
          }`}
          title={selectionMode ? "退出选择" : "批量选择"}
        >
          <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
            <path strokeLinecap="round" strokeLinejoin="round" d="M9 12.75 11.25 15 15 9.75M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0Z" />
          </svg>
        </button>

        {/* More menu */}
        <div className="relative">
          <button
            onClick={() => setShowMoreMenu((v) => !v)}
            className="flex-shrink-0 rounded-lg p-1.5 text-[var(--color-text-muted)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)] transition-colors"
            title="更多"
          >
            <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M6.75 12a.75.75 0 1 1-1.5 0 .75.75 0 0 1 1.5 0ZM12.75 12a.75.75 0 1 1-1.5 0 .75.75 0 0 1 1.5 0ZM18.75 12a.75.75 0 1 1-1.5 0 .75.75 0 0 1 1.5 0Z" />
            </svg>
          </button>

          {showMoreMenu && (
            <>
              <div className="fixed inset-0 z-[59]" onClick={() => setShowMoreMenu(false)} />
              <div className="absolute right-0 top-full z-[60] mt-1 min-w-[170px] rounded-xl border border-[var(--color-border)] bg-[var(--color-bg-elevated)]/95 backdrop-blur-xl py-1.5 shadow-2xl shadow-black/30 animate-scale-in">
                {/* Group by */}
                <div className="px-2 pb-1.5">
                  <p className="px-2 pb-1 text-[10px] font-semibold uppercase tracking-wider text-[var(--color-text-muted)]">分组</p>
                  <button onClick={() => { setGroupBy("none"); setShowMoreMenu(false); }} className={`flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-xs transition-colors ${groupBy === "none" ? "bg-[var(--color-accent-soft)] text-[var(--color-accent)]" : "text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"}`}>平铺</button>
                  <button onClick={() => { setGroupBy("date"); setShowMoreMenu(false); }} className={`flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-xs transition-colors ${groupBy === "date" ? "bg-[var(--color-accent-soft)] text-[var(--color-accent)]" : "text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"}`}>按日期分组</button>
                </div>

                {/* Sort field */}
                <div className="px-2 pb-1.5">
                  <p className="px-2 pb-1 text-[10px] font-semibold uppercase tracking-wider text-[var(--color-text-muted)]">排序</p>
                  {(["imported_at", "created_at", "modified_at", "file_size", "width", "height"] as SortField[]).map((f) => (
                    <button key={f} onClick={() => { setSortBy(f); setShowMoreMenu(false); }} className={`flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-xs transition-colors ${sortBy === f ? "bg-[var(--color-accent-soft)] text-[var(--color-accent)]" : "text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"}`}>
                      {{imported_at: "导入时间", created_at: "创建时间", modified_at: "修改时间", file_size: "文件大小", width: "宽度", height: "高度"}[f]}
                    </button>
                  ))}
                </div>

                <div className="my-1 border-t border-[var(--color-border)]" />

                {/* Actions */}
                <button onClick={() => { setShowExportDialog(true); setShowMoreMenu(false); }} disabled={media.length === 0} className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-[var(--color-text-secondary)] transition-colors hover:bg-[var(--color-bg-hover)] hover:text-[var(--color-text-primary)] disabled:opacity-50">
                  <svg className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}><path strokeLinecap="round" strokeLinejoin="round" d="M3 16.5v2.25A2.25 2.25 0 0 0 5.25 21h13.5A2.25 2.25 0 0 0 21 18.75V16.5M16.5 12 12 16.5m0 0L7.5 12m4.5 4.5V3" /></svg>
                  导出
                </button>
                <button onClick={async () => { setShowMoreMenu(false); setFindingDups(true); try { const groups = await mediaFindDuplicates(); setDupGroups(groups); setShowDupDialog(true); } catch (e) { console.error("Failed to find duplicates:", e); } finally { setFindingDups(false); } }} disabled={findingDups} className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-[var(--color-text-secondary)] transition-colors hover:bg-[var(--color-bg-hover)] hover:text-[var(--color-text-primary)] disabled:opacity-50">
                  <svg className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}><path strokeLinecap="round" strokeLinejoin="round" d="M21 21l-5.197-5.197m0 0A7.5 7.5 0 1 0 5.196 5.196a7.5 7.5 0 0 0 10.607 10.607Z" /></svg>
                  {findingDups ? "分析中..." : "查找重复"}
                </button>
                <button onClick={() => { setShowImportDialog(true); setShowMoreMenu(false); }} className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-[var(--color-text-secondary)] transition-colors hover:bg-[var(--color-bg-hover)] hover:text-[var(--color-text-primary)]">
                  <svg className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}><path strokeLinecap="round" strokeLinejoin="round" d="M19.5 14.25v-2.625a3.375 3.375 0 0 0-3.375-3.375h-1.5A1.125 1.125 0 0 1 13.5 7.125v-1.5a3.375 3.375 0 0 0-3.375-3.375H8.25m6.75 12H9m1.5-12H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 0 0-9-9Z" /></svg>
                  导入 ZIP
                </button>
              </div>
            </>
          )}
        </div>

        {/* Item count */}
        <span className="flex-shrink-0 text-xs tabular-nums text-[var(--color-text-muted)]">{media.length} 项</span>

        {/* AI badge */}
        {aiRemaining > 0 && (
          <span className="flex-shrink-0 rounded-full bg-[var(--color-accent-soft)] px-2 py-0.5 text-[11px] text-[var(--color-accent)] whitespace-nowrap">
            AI · {aiRemaining}
          </span>
        )}
      </div>

      {/* Import message */}
      {importMessage && (
        <div
          className={`px-6 py-2 text-xs ${
            isImporting
              ? "bg-[var(--color-accent-soft)] text-[var(--color-accent)]"
              : importMessage.includes("失败")
              ? "bg-red-900/30 text-red-400"
              : "bg-green-900/30 text-green-400"
          }`}
        >
          <div className="flex items-center justify-between">
            <span>{importMessage}</span>
            {importProgress && isImporting && (
              <span>{importProgress.current} / {importProgress.total}</span>
            )}
          </div>
          {importProgress && isImporting && (
            <div className="mt-1 h-1 w-full rounded-full bg-[var(--color-accent-soft)]">
              <div
                className="h-full rounded-full bg-[var(--color-accent)] transition-all duration-150"
                style={{ width: `${(importProgress.current / importProgress.total) * 100}%` }}
              />
            </div>
          )}
        </div>
      )}

      {/* Batch action bar */}
      {selectionMode && selectedIds.size > 0 && (
        <div className="flex items-center justify-between border-b border-[var(--color-border)] bg-[var(--color-bg-tertiary)]/80 px-6 py-2">
          <span className="text-sm text-[var(--color-text-secondary)]">
            已选择 <span className="font-bold text-green-400">{selectedIds.size}</span> 项
          </span>
          <div className="flex items-center gap-2">
            <button
              onClick={handleSelectAll}
              className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
            >
              全选
            </button>
            <button
              onClick={async () => {
                const all = await loadCollections();
                setCollectionsForPicker(all);
                setAddToCollectionMediaIds(Array.from(selectedIds));
                setCollectionPickerSearch("");
                setShowAddToCollection(true);
              }}
              className="rounded bg-purple-600 px-3 py-1 text-xs font-medium text-white hover:bg-purple-500"
            >
              添加到集合
            </button>
            <button
              onClick={() => setShowBatchTagDialog(true)}
              className="rounded bg-[var(--color-accent)] px-3 py-1 text-xs font-medium text-white hover:bg-[var(--color-accent-hover)]"
            >
              添加标签
            </button>
            <button
              onClick={async () => {
                setBatchRemoveTagSearch("");
                setIntersectTags([]);
                setShowBatchRemoveTagDialog(true);
                const tags = await mediaTagsIntersect(Array.from(selectedIds));
                setIntersectTags(tags);
              }}
              className="rounded border border-orange-800/50 bg-orange-900/20 px-3 py-1 text-xs text-orange-400 hover:bg-orange-900/30"
            >
              移除标签
            </button>
            <button
              onClick={() => setShowBatchCaptionDialog(true)}
              className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-3 py-1 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
            >
              添加描述
            </button>
            <button
              onClick={handleBatchDelete}
              className="rounded border border-red-800/50 bg-red-900/20 px-3 py-1 text-xs text-red-400 hover:bg-red-900/30"
            >
              删除
            </button>
            <button
              onClick={() => setSelectedIds(new Set())}
              className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
            >
              取消全选
            </button>
          </div>
        </div>
      )}

      {/* Content */}
      <div className="flex flex-1 overflow-hidden">
        <div className="flex flex-1 flex-col p-4">
          {media.length === 0 && !debouncedSearch ? (
            <DropZone dropHover={dropHover} />
          ) : media.length === 0 && debouncedSearch ? (
            <div className="flex flex-col items-center justify-center py-20">
              <svg
                className="mb-4 h-10 w-10 text-[var(--color-text-muted)]"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                strokeWidth={1.5}
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  d="m21 21-5.197-5.197m0 0A7.5 7.5 0 1 0 5.196 5.196a7.5 7.5 0 0 0 10.607 10.607Z"
                />
              </svg>
              <p className="text-sm text-[var(--color-text-muted)]">没有找到匹配的媒体</p>
              <p className="mt-1 text-xs text-[var(--color-text-muted)]">
                试试修改搜索条件，或
                <button
                  onClick={() => { setSearchQuery(""); setSearchParams({}); }}
                  className="ml-1 text-[var(--color-accent)] hover:text-[var(--color-accent-hover)]"
                >
                  重置搜索
                </button>
              </p>
            </div>
          ) : viewMode === "table" ? (
            <TableView
              media={media}
              groups={groups}
              selectedId={selected?.id ?? null}
              onSelect={setSelected}
              onDoubleClick={(item) => {
                const idx = media.findIndex((m) => m.id === item.id);
                if (idx >= 0) setLightboxIndex(idx);
              }}
              onContextMenu={(e, item) => {
                e.preventDefault();
                setCtxMenu({ x: e.clientX, y: e.clientY, media: item });
              }}
              sortBy={sortBy}
              descending={descending}
              onSortChange={(field) => {
                if (sortBy === field) {
                  setDescending((d) => !d);
                } else {
                  setSortBy(field);
                  setDescending(true);
                }
              }}
              selectedIds={Array.from(selectedIds)}
              selectionMode={selectionMode}
              onToggleSelect={(item, index, shiftKey) => handleToggleSelect(item, index, shiftKey)}
              onAddToCollection={(item) => {
                setAddToCollectionMediaIds([item.id]);
                setShowAddToCollection(true);
              }}
              onDelete={(item) => {
                setPendingDeleteId(item.id);
                setDeleteConfirm("single");
              }}
            />
          ) : (
            <Gallery
              media={media}
              groups={groups}
              selectedId={selected?.id ?? null}
              onSelect={setSelected}
              onDoubleClick={(item) => {
                const idx = media.findIndex((m) => m.id === item.id);
                if (idx >= 0) setLightboxIndex(idx);
              }}
              onContextMenu={(e, item) => {
                e.preventDefault();
                setCtxMenu({ x: e.clientX, y: e.clientY, media: item });
              }}
              selectedIds={Array.from(selectedIds)}
              selectionMode={selectionMode}
              onToggleSelect={(item, index, shiftKey) => handleToggleSelect(item, index, shiftKey)}
            />
          )}
        </div>
        <DetailPanel
          media={selected}
          collapsed={detailCollapsed}
          onToggleCollapse={() => setDetailCollapsed(!detailCollapsed)}
          onDeleted={() => { setSelected(null); setDetailCollapsed(false); loadMedia(); }}
        />
      </div>

      {/* Batch tag dialog */}
      {showBatchTagDialog && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
          onClick={() => setShowBatchTagDialog(false)}
        >
          <div
            className="w-80 rounded-lg border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] p-4 shadow-xl"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 className="mb-3 text-sm font-bold text-[var(--color-text-primary)]">
              添加标签到 {selectedIds.size} 张图片
            </h3>
            <input
              type="text"
              value={batchTagSearch}
              onChange={(e) => setBatchTagSearch(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !hasExactMatch && batchTagSearch.trim()) {
                  handleCreateAndBatchAdd();
                }
              }}
              placeholder="搜索或新建标签..."
              autoFocus
              className="mb-3 w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)] focus:border-[var(--color-accent)]"
            />
            <div className="max-h-48 overflow-auto space-y-1">
              {filteredBatchTags.length === 0 && !batchTagSearch.trim() && (
                <p className="py-2 text-center text-xs text-[var(--color-text-muted)]">暂无标签</p>
              )}
              {filteredBatchTags.map((tag) => (
                <button
                  key={tag.id}
                  onClick={() => handleBatchTagAdd(tag.id)}
                  className="block w-full rounded px-2 py-1.5 text-left text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
                >
                  {tag.name}
                </button>
              ))}
              {batchTagSearch.trim() && !hasExactMatch && (
                <button
                  onClick={handleCreateAndBatchAdd}
                  className="block w-full rounded bg-[var(--color-accent-soft)] px-2 py-1.5 text-left text-xs text-[var(--color-accent)] hover:bg-[var(--color-accent-soft-hover)]"
                >
                  创建标签 "{batchTagSearch.trim()}" 并添加
                </button>
              )}
            </div>
            <div className="mt-3 flex justify-end">
              <button
                onClick={() => {
                  setShowBatchTagDialog(false);
                  setBatchTagSearch("");
                }}
                className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-3 py-1 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
              >
                取消
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Batch caption dialog */}
      {showBatchCaptionDialog && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
          onClick={() => setShowBatchCaptionDialog(false)}
        >
          <div
            className="w-96 rounded-lg border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] p-4 shadow-xl"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 className="mb-3 text-sm font-bold text-[var(--color-text-primary)]">
              添加描述到 {selectedIds.size} 张图片
            </h3>
            <textarea
              value={batchCaptionText}
              onChange={(e) => setBatchCaptionText(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
                  e.preventDefault();
                  if (batchCaptionText.trim()) {
                    captionCreateBatch(Array.from(selectedIds), batchCaptionText.trim()).then(() => {
                      setShowBatchCaptionDialog(false);
                      setBatchCaptionText("");
                      setSelectedIds(new Set());
                      setSelectionMode(false);
                    });
                  }
                }
              }}
              placeholder="输入描述文本... (Ctrl+Enter 保存)"
              rows={4}
              autoFocus
              className="mb-3 w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)] focus:border-[var(--color-accent)]"
            />
            <div className="flex justify-end gap-2">
              <button
                onClick={() => {
                  setShowBatchCaptionDialog(false);
                  setBatchCaptionText("");
                }}
                className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-3 py-1 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
              >
                取消
              </button>
              <button
                onClick={async () => {
                  if (!batchCaptionText.trim()) return;
                  await captionCreateBatch(Array.from(selectedIds), batchCaptionText.trim());
                  setShowBatchCaptionDialog(false);
                  setBatchCaptionText("");
                  setSelectedIds(new Set());
                  setSelectionMode(false);
                }}
                disabled={!batchCaptionText.trim()}
                className="rounded bg-[var(--color-accent)] px-3 py-1 text-xs font-medium text-white hover:bg-[var(--color-accent-hover)] disabled:opacity-50"
              >
                添加
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Batch remove tag dialog */}
      {showBatchRemoveTagDialog && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
          onClick={() => setShowBatchRemoveTagDialog(false)}
        >
          <div
            className="w-80 rounded-lg border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] p-4 shadow-xl"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 className="mb-3 text-sm font-bold text-[var(--color-text-primary)]">
              从 {selectedIds.size} 张图片移除标签
            </h3>
            <input
              type="text"
              value={batchRemoveTagSearch}
              onChange={(e) => setBatchRemoveTagSearch(e.target.value)}
              placeholder="搜索标签..."
              autoFocus
              className="mb-3 w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)] focus:border-[var(--color-accent)]"
            />
            <div className="max-h-48 overflow-auto space-y-1">
              {intersectTags.length === 0 ? (
                <p className="py-2 text-center text-xs text-[var(--color-text-muted)]">
                  {intersectTags.length === 0 && batchRemoveTagSearch === "" ? "所选图片没有共同标签" : "未找到匹配标签"}
                </p>
              ) : (
                intersectTags
                  .filter((t) =>
                    t.name.toLowerCase().includes(batchRemoveTagSearch.trim().toLowerCase())
                  )
                  .map((tag) => (
                    <button
                      key={tag.id}
                      onClick={async () => {
                        await mediaTagRemoveBatch(Array.from(selectedIds), tag.id);
                        setShowBatchRemoveTagDialog(false);
                        setBatchRemoveTagSearch("");
                        setSelectedIds(new Set());
                        setSelectionMode(false);
                      }}
                      className="block w-full rounded px-2 py-1.5 text-left text-xs text-[var(--color-text-secondary)] hover:bg-red-900/20 hover:text-red-400"
                    >
                      移除 "{tag.name}"
                    </button>
                  ))
              )}
            </div>
            <div className="mt-3 flex justify-end">
              <button
                onClick={() => {
                  setShowBatchRemoveTagDialog(false);
                  setBatchRemoveTagSearch("");
                }}
                className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-3 py-1 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
              >
                取消
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Add to collection dialog */}
      {showAddToCollection && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
          onClick={() => setShowAddToCollection(false)}
        >
          <div
            className="w-80 rounded-lg border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] p-4 shadow-xl"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 className="mb-3 text-sm font-bold text-[var(--color-text-primary)]">
              添加到集合 ({addToCollectionMediaIds.length} 张)
            </h3>
            <input
              type="text"
              value={collectionPickerSearch}
              onChange={(e) => setCollectionPickerSearch(e.target.value)}
              placeholder="搜索集合..."
              autoFocus
              className="mb-3 w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)]"
            />
            <div className="max-h-48 overflow-auto space-y-1">
              {collectionsForPicker
                .filter((c) => c.name.toLowerCase().includes(collectionPickerSearch.toLowerCase()))
                .map((c) => (
                  <button
                    key={c.id}
                    onClick={async () => {
                      await collectionAddBatch(c.id, addToCollectionMediaIds);
                      setShowAddToCollection(false);
                      setAddToCollectionMediaIds([]);
                      window.dispatchEvent(new CustomEvent("collections-changed"));
                    }}
                    className="block w-full rounded px-2 py-1.5 text-left text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
                  >
                    {c.name} ({c.item_count ?? 0})
                  </button>
                ))}
              {collectionsForPicker.filter((c) => c.name.toLowerCase().includes(collectionPickerSearch.toLowerCase())).length === 0 && (
                <p className="py-2 text-center text-xs text-[var(--color-text-muted)]">
                  未找到匹配的集合
                </p>
              )}
            </div>
            <div className="mt-3 flex justify-end">
              <button
                onClick={() => setShowAddToCollection(false)}
                className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-3 py-1 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
              >
                取消
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Save filter dialog */}
      {showSaveDialog && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
          onClick={() => setShowSaveDialog(false)}
        >
          <div
            className="w-80 rounded-lg border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] p-4 shadow-xl"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 className="mb-3 text-sm font-bold text-[var(--color-text-primary)]">保存筛选器</h3>
            <p className="mb-1 text-[10px] text-[var(--color-text-muted)]">查询条件</p>
            <p className="mb-3 rounded bg-[var(--color-bg-secondary)] px-2 py-1.5 text-xs text-[var(--color-text-secondary)] font-mono">
              {debouncedSearch}
            </p>
            <label className="mb-1 block text-[10px] text-[var(--color-text-muted)]">名称</label>
            <input
              type="text"
              value={savedFilterName}
              onChange={(e) => setSavedFilterName(e.target.value)}
              onKeyDown={async (e) => {
                if (e.key === "Enter" && savedFilterName.trim()) {
                  await savedFiltersSave(savedFilterName.trim(), debouncedSearch);
                  window.dispatchEvent(new CustomEvent("saved-filters-changed"));
                  setShowSaveDialog(false);
                  setSavedFilterName("");
                }
              }}
              placeholder="输入筛选器名称..."
              autoFocus
              className="mb-3 w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)] focus:border-[var(--color-accent)]"
            />
            <div className="flex justify-end gap-2">
              <button
                onClick={() => {
                  setShowSaveDialog(false);
                  setSavedFilterName("");
                }}
                className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-3 py-1 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
              >
                取消
              </button>
              <button
                onClick={async () => {
                  if (!savedFilterName.trim()) return;
                  await savedFiltersSave(savedFilterName.trim(), debouncedSearch);
                  window.dispatchEvent(new CustomEvent("saved-filters-changed"));
                  setShowSaveDialog(false);
                  setSavedFilterName("");
                }}
                disabled={!savedFilterName.trim()}
                className="rounded bg-[var(--color-accent)] px-3 py-1 text-xs font-medium text-white hover:bg-[var(--color-accent-hover)] disabled:opacity-50"
              >
                保存
              </button>
            </div>
          </div>
        </div>
      )}
      {/* Duplicate finder dialog */}
      {showDupDialog && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
          onClick={() => setShowDupDialog(false)}
        >
          <div
            className="max-h-[80vh] w-[500px] overflow-auto rounded-lg border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] p-5 shadow-xl"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 className="mb-4 text-sm font-bold text-[var(--color-text-primary)]">
              相似图片（pHash 汉明距离 ≤ 10）
            </h3>
            {dupGroups.length === 0 ? (
              <p className="text-xs text-[var(--color-text-muted)]">未发现重复图片</p>
            ) : (
              <div className="space-y-3">
                {dupGroups.map((group, i) => (
                  <div key={i} className="rounded border border-[var(--color-border-light)] p-3">
                    <p className="mb-2 text-xs text-[var(--color-text-muted)]">
                      组 {i + 1} — {group.length} 张相似图片
                    </p>
                    <div className="max-h-48 overflow-auto space-y-1">
                      {group.map((m) => (
                        <div
                          key={m.id}
                          className="flex items-center justify-between rounded bg-[var(--color-bg-secondary)] px-2 py-1"
                        >
                          <span className="text-xs text-[var(--color-text-secondary)] truncate">
                            {m.source_path?.split("\\").pop() || m.id}
                          </span>
                          <span className="text-[10px] text-[var(--color-text-muted)]">
                            {m.width}×{m.height}
                          </span>
                        </div>
                      ))}
                    </div>
                  </div>
                ))}
              </div>
            )}
            <div className="mt-4 flex justify-end">
              <button
                onClick={() => setShowDupDialog(false)}
                className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] px-3 py-1.5 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
              >
                关闭
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Export dialog */}
      {showExportDialog && (
        <ExportDialog
          mediaIds={Array.from(selectedIds)}
          totalCount={media.length}
          onClose={() => setShowExportDialog(false)}
        />
      )}

      {/* Context menu */}
      {ctxMenu && (
        <div
          className="fixed z-[60] min-w-[120px] rounded-lg border border-[var(--color-border)] bg-[var(--color-bg-tertiary)] py-1 shadow-xl"
          style={{ left: ctxMenu.x, top: ctxMenu.y }}
          onClick={(e) => e.stopPropagation()}
        >
          <button
            onClick={() => {
              const idx = media.findIndex((m) => m.id === ctxMenu.media.id);
              if (idx >= 0) setLightboxIndex(idx);
              setCtxMenu(null);
            }}
            className="block w-full px-3 py-1.5 text-left text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
          >
            查看原图
          </button>
          <button
            onClick={() => {
              setSelected(ctxMenu.media);
              setCtxMenu(null);
            }}
            className="block w-full px-3 py-1.5 text-left text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
          >
            查看详情
          </button>
          <div className="my-1 border-t border-[var(--color-border)]" />
          <button
            onClick={() => {
              setShowBatchTagDialog(true);
              setSelectedIds(new Set([ctxMenu.media.id]));
              setSelectionMode(true);
              setCtxMenu(null);
            }}
            className="block w-full px-3 py-1.5 text-left text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
          >
            添加标签
          </button>
          <button
            onClick={async () => {
              const all = await loadCollections();
              setCollectionsForPicker(all);
              setAddToCollectionMediaIds([ctxMenu.media.id]);
              setCollectionPickerSearch("");
              setShowAddToCollection(true);
              setCtxMenu(null);
            }}
            className="block w-full px-3 py-1.5 text-left text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
          >
            添加到集合
          </button>
          {collectionId && (
            <button
              onClick={async () => {
                await removeFromCollection(collectionId, ctxMenu.media.id);
                setCtxMenu(null);
                loadMedia();
                window.dispatchEvent(new CustomEvent("collections-changed"));
              }}
              className="block w-full px-3 py-1.5 text-left text-xs text-orange-400 hover:bg-orange-900/20"
            >
              从集合移除
            </button>
          )}
          <div className="my-1 border-t border-[var(--color-border)]" />
          <button
            onClick={() => {
              setPendingDeleteId(ctxMenu.media.id);
              setCtxMenu(null);
              setDeleteConfirm("single");
            }}
            className="block w-full px-3 py-1.5 text-left text-xs text-[var(--color-danger)] hover:bg-[var(--color-danger-soft)]"
          >
            删除
          </button>
        </div>
      )}

      {/* Click-away to close context menu */}
      {ctxMenu && (
        <div
          className="fixed inset-0 z-[59]"
          onClick={() => setCtxMenu(null)}
          onContextMenu={(e) => { e.preventDefault(); setCtxMenu(null); }}
        />
      )}

      {/* Lightbox */}
      {lightboxIndex !== null && (
        <Lightbox
          media={media}
          currentIndex={lightboxIndex}
          onClose={() => setLightboxIndex(null)}
          onNavigate={(idx) => setLightboxIndex(idx)}
        />
      )}

      {/* Import ZIP dialog */}
      {showImportDialog && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
          onClick={() => {
            setShowImportDialog(false);
            setImportResult(null);
          }}
        >
          <div
            className="w-80 rounded-lg border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] p-5 shadow-xl"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 className="mb-4 text-sm font-bold text-[var(--color-text-primary)]">
              导入 ZIP 数据集
            </h3>

            {importResult ? (
              <div className="space-y-3">
                <div className="rounded bg-green-900/20 p-3">
                  <p className="text-xs text-green-400">{importResult}</p>
                </div>
                <button
                  onClick={() => {
                    setShowImportDialog(false);
                    setImportResult(null);
                  }}
                  className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] px-3 py-1.5 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
                >
                  关闭
                </button>
              </div>
            ) : (
              <>
                <label className="mb-1 block text-xs text-[var(--color-text-muted)]">
                  ZIP 文件路径
                </label>
                <div className="mb-4 flex gap-1.5">
                  <input
                    type="text"
                    value={importZipPath}
                    onChange={(e) => setImportZipPath(e.target.value)}
                    placeholder="C:\Users\...\export.zip"
                    className="flex-1 rounded border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)]"
                  />
                  <button
                    onClick={async () => {
                      const selected = await open({
                        multiple: false,
                        filters: [{ name: "ZIP", extensions: ["zip"] }],
                      });
                      if (selected) setImportZipPath(selected);
                    }}
                    className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] px-2 py-1.5 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
                    title="选择文件"
                  >
                    ...
                  </button>
                </div>
                <p className="mb-4 text-[10px] text-[var(--color-text-muted)]">
                  导入 ZIP 中的图片及同名 .json 元数据（caption + tags）
                </p>
                <div className="flex justify-end gap-2">
                  <button
                    onClick={() => {
                      setShowImportDialog(false);
                      setImportResult(null);
                    }}
                    className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] px-3 py-1.5 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
                  >
                    取消
                  </button>
                  <button
                    onClick={async () => {
                      if (!importZipPath.trim()) return;
                      setImporting(true);
                      try {
                        const count = await importZip(importZipPath.trim());
                        setImportResult(`成功导入 ${count} 张图片`);
                        setImportZipPath("");
                        loadMedia();
                      } catch (e) {
                        setImportResult(`导入失败: ${e}`);
                      } finally {
                        setImporting(false);
                      }
                    }}
                    disabled={!importZipPath.trim() || importing}
                    className="rounded bg-[var(--color-accent)] px-4 py-1.5 text-xs font-medium text-white hover:bg-[var(--color-accent-hover)] disabled:opacity-50"
                  >
                    {importing ? "导入中..." : "开始导入"}
                  </button>
                </div>
              </>
            )}
          </div>
        </div>
      )}
      <ConfirmDialog
        open={deleteConfirm === "batch"}
        title="批量删除"
        message={`确定要删除 ${selectedIds.size} 张图片吗？可以在回收站中恢复。`}
        variant="danger"
        confirmLabel="删除"
        onConfirm={confirmBatchDelete}
        onCancel={() => setDeleteConfirm(null)}
      />
      <ConfirmDialog
        open={deleteConfirm === "single"}
        title="删除图片"
        message="确定要删除这张图片吗？可以在回收站中恢复。"
        variant="danger"
        confirmLabel="删除"
        onConfirm={async () => {
          if (!pendingDeleteId) return;
          try {
            await mediaSoftDelete(pendingDeleteId);
            setSelected(null);
            loadMedia();
            window.dispatchEvent(new CustomEvent("collections-changed"));
          } catch (e) {
            console.error("Failed to delete:", e);
          } finally {
            setDeleteConfirm(null);
            setPendingDeleteId(null);
          }
        }}
        onCancel={() => { setDeleteConfirm(null); setPendingDeleteId(null); }}
      />
    </div>
  );
}

export default AllMedia;
