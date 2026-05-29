import { useCallback, useEffect, useMemo, useRef, useState } from "react";
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
  savedFiltersSave,
  settingsGet,
  settingsSet,
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
  const [viewMode, setViewMode] = useState<ViewMode>(
    () => (sessionStorage.getItem("view_mode") as ViewMode) || "grid"
  );
  const [sortBy, setSortBy] = useState<SortField>(
    () => (sessionStorage.getItem("sort_by") as SortField) || "imported_at"
  );
  const [descending, setDescending] = useState(
    () => sessionStorage.getItem("sort_desc") !== "false"
  );
  const [gridScale, setGridScale] = useState(
    () => Number(sessionStorage.getItem("grid_scale")) || 1
  );
  const [groupBy, setGroupBy] = useState<GroupMode>(
    () => (sessionStorage.getItem("group_by") as GroupMode) || "none"
  );

  // Load saved preferences
  useEffect(() => {
    settingsGet("group_by").then((v) => {
      if (v === "date" || v === "none") setGroupBy(v);
    }).catch(() => {});
    settingsGet("sort_by").then((v) => {
      if (v) setSortBy(v as SortField);
    }).catch(() => {});
    settingsGet("sort_desc").then((v) => {
      setDescending(v !== "false");
    }).catch(() => {});
    settingsGet("grid_scale").then((v) => {
      if (v) setGridScale(Number(v));
    }).catch(() => {});
    settingsGet("view_mode").then((v) => {
      if (v === "grid" || v === "table") setViewMode(v);
    }).catch(() => {});
  }, []);

  // Persist preferences
  useEffect(() => {
    sessionStorage.setItem("group_by", groupBy);
    settingsSet("group_by", groupBy).catch(() => {});
  }, [groupBy]);
  useEffect(() => {
    sessionStorage.setItem("sort_by", sortBy);
    settingsSet("sort_by", sortBy).catch(() => {});
  }, [sortBy]);
  useEffect(() => {
    sessionStorage.setItem("sort_desc", String(descending));
    settingsSet("sort_desc", String(descending)).catch(() => {});
  }, [descending]);
  useEffect(() => {
    sessionStorage.setItem("view_mode", viewMode);
    settingsSet("view_mode", viewMode).catch(() => {});
  }, [viewMode]);
  useEffect(() => {
    sessionStorage.setItem("grid_scale", String(gridScale));
    settingsSet("grid_scale", String(gridScale)).catch(() => {});
  }, [gridScale]);

  // Context menu
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number; media: Media } | null>(null);
  const [showMoreMenu, setShowMoreMenu] = useState(false);

  // Add to collection dialog (shared between context menu and batch)
  const [showAddToCollection, setShowAddToCollection] = useState(false);
  const [addToCollectionMediaIds, setAddToCollectionMediaIds] = useState<string[]>([]);
  const [collectionPickerSearch, setCollectionPickerSearch] = useState("");
  const [collectionsForPicker, setCollectionsForPicker] = useState<Collection[]>([]);

  // Multi-select
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

  // Keep for future use
  void setShowBatchRemoveTagDialog;
  void setBatchRemoveTagSearch;
  void setIntersectTags;

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

  // When grouping by date, sort media by imported_at descending for correct grouping
  const displayMedia = useMemo(() => {
    if (groupBy !== "date") return media;
    return [...media].sort((a, b) => {
      const da = a.imported_at ?? "";
      const db = b.imported_at ?? "";
      return db.localeCompare(da);
    });
  }, [media, groupBy]);

  // Compute date groups from displayMedia (humanized labels)
  const groups = groupBy === "date"
    ? (() => {
        const result: GroupInfo[] = [];
        let cur = "";
        const now = new Date();
        const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
        const weekAgo = new Date(today.getTime() - 7 * 86400000);

        const fmtLabel = (dateStr: string) => {
          if (dateStr === "未知日期") return dateStr;
          const d = new Date(dateStr);
          if (isNaN(d.getTime())) return dateStr;
          const dDay = new Date(d.getFullYear(), d.getMonth(), d.getDate());
          const diff = Math.round((today.getTime() - dDay.getTime()) / 86400000);
          if (diff === 0) return "今天";
          if (diff === 1) return "昨天";
          if (diff < 7 && dDay > weekAgo) return "本周";
          if (diff < 14) return "上周";
          if (d.getFullYear() === now.getFullYear()) {
            return `${d.getFullYear()}年${d.getMonth() + 1}月`;
          }
          return `${d.getFullYear()}年${d.getMonth() + 1}月`;
        };

        for (let i = 0; i < displayMedia.length; i++) {
          const raw = displayMedia[i].imported_at?.slice(0, 10) ?? "未知日期";
          const label = fmtLabel(raw);
          if (label !== cur) {
            cur = label;
            result.push({ label, startIndex: i, count: 0 });
          }
          result[result.length - 1].count++;
        }
        return result;
      })()
    : [];

  // Keyboard shortcuts
  const selectedIdsRef2 = useRef(selectedIds);
  selectedIdsRef2.current = selectedIds;
  const displayMediaRef2 = useRef(displayMedia);
  displayMediaRef2.current = displayMedia;

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;

      if (e.key === "Escape") {
        if (selectedIdsRef2.current.size > 0) {
          setSelectedIds(new Set());
        } else {
          setSelected(null);
        }
      }
      if (e.key === "a" && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        const m = displayMediaRef2.current;
        if (m.length > 0) {
          setSelectedIds(new Set(m.map((x) => x.id)));
        }
      }
      if (e.key === "Delete" && selectedIdsRef2.current.size > 0) {
        e.preventDefault();
        setDeleteConfirm("batch");
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  // Listen for display variant changes from DetailPanel
  const selectedRefForVariant = useRef(selected);
  selectedRefForVariant.current = selected;
  useEffect(() => {
    const handler = (e: Event) => {
      const detail = (e as CustomEvent).detail as { mediaId: string; variantId: string | null };
      setMedia((prev) =>
        prev.map((m) =>
          m.id === detail.mediaId
            ? { ...m, display_variant_id: detail.variantId }
            : m,
        ),
      );
      if (selectedRefForVariant.current?.id === detail.mediaId) {
        setSelected((prev) => prev ? { ...prev, display_variant_id: detail.variantId } : null);
      }
    };
    window.addEventListener("display-variant-changed", handler);
    return () => window.removeEventListener("display-variant-changed", handler);
  }, []);

  const [lastSelectedIndex, setLastSelectedIndex] = useState<number | null>(null);

  // Keep lastSelectedIndex in sync with single-select (card click without modifiers)
  useEffect(() => {
    if (selected) {
      const idx = displayMedia.findIndex((m) => m.id === selected.id);
      if (idx >= 0) setLastSelectedIndex(idx);
    }
  }, [selected?.id, displayMedia]);

  const handleToggleSelect = (item: Media, index: number, shiftKey: boolean) => {
    if (shiftKey && lastSelectedIndex !== null) {
      // Range select from lastSelectedIndex to index
      const start = Math.min(lastSelectedIndex, index);
      const end = Math.max(lastSelectedIndex, index);
      const rangeIds = displayMedia.slice(start, end + 1).map((m) => m.id);
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
    setSelectedIds(new Set(displayMedia.map((m) => m.id)));
  };

  const handleBatchTagAdd = async (tagId: string) => {
    if (selectedIds.size === 0) return;
    try {
      await mediaTagAddBatch(Array.from(selectedIds), tagId);
      setShowBatchTagDialog(false);
      setBatchTagSearch("");
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
        setDeleteConfirm(null);
    loadMedia();
    window.dispatchEvent(new CustomEvent("collections-changed"));
    showToast(`已删除 ${selectedIds.size} 张图片`);
  };

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
    <div className="relative flex h-full flex-col">
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

        {/* Grid zoom (only in grid mode) */}
        {viewMode === "grid" && (
          <div className="flex items-center gap-0.5">
            <button
              onClick={() => setGridScale((s) => Math.max(0.5, s - 0.1))}
              className="rounded p-1 text-[var(--color-text-muted)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)] transition-colors disabled:opacity-30"
              disabled={gridScale <= 0.5}
              title="缩小"
            >
              <svg className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M5 12h14" />
              </svg>
            </button>
            <button
              onClick={() => setGridScale(1)}
              className="rounded px-1 py-0.5 text-[11px] tabular-nums text-[var(--color-text-muted)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)] transition-colors"
              title="重置缩放"
            >
              {Math.round(gridScale * 100)}%
            </button>
            <button
              onClick={() => setGridScale((s) => Math.min(2, s + 0.1))}
              className="rounded p-1 text-[var(--color-text-muted)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)] transition-colors disabled:opacity-30"
              disabled={gridScale >= 2}
              title="放大"
            >
              <svg className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M12 5v14m7-7H5" />
              </svg>
            </button>
          </div>
        )}

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
                <button onClick={() => { setShowExportDialog(true); setShowMoreMenu(false); }} disabled={displayMedia.length === 0} className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-[var(--color-text-secondary)] transition-colors hover:bg-[var(--color-bg-hover)] hover:text-[var(--color-text-primary)] disabled:opacity-50">
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

      {/* Content */}
      <div className="flex flex-1 overflow-hidden">
        <div className="flex flex-1 flex-col overflow-hidden p-4">
          {displayMedia.length === 0 && !debouncedSearch ? (
            <DropZone dropHover={dropHover} />
          ) : displayMedia.length === 0 && debouncedSearch ? (
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
              media={displayMedia}
              groups={groups}
              selectedId={selected?.id ?? null}
              onSelect={setSelected}
              onDoubleClick={(item) => {
                const idx = displayMedia.findIndex((m) => m.id === item.id);
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
              media={displayMedia}
              groups={groups}
              scale={gridScale}
              selectedId={selected?.id ?? null}
              onSelect={setSelected}
              onDoubleClick={(item) => {
                const idx = displayMedia.findIndex((m) => m.id === item.id);
                if (idx >= 0) setLightboxIndex(idx);
              }}
              onContextMenu={(e, item) => {
                e.preventDefault();
                setCtxMenu({ x: e.clientX, y: e.clientY, media: item });
              }}
              selectedIds={Array.from(selectedIds)}
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
            onClick={handleSelectAll}
            className="rounded-lg px-3 py-1.5 text-xs font-medium text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-tertiary)] transition-colors"
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
            className="rounded-lg p-1.5 text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)] transition-colors"
            title="添加到集合"
          >
            <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M12 10.5v6m3-3H9m4.06-7.19-2.12-2.12a1.5 1.5 0 0 0-1.061-.44H4.5A2.25 2.25 0 0 0 2.25 6v12a2.25 2.25 0 0 0 2.25 2.25h15A2.25 2.25 0 0 0 21.75 18V9a2.25 2.25 0 0 0-2.25-2.25h-5.379a1.5 1.5 0 0 1-1.06-.44Z" />
            </svg>
          </button>
          <button
            onClick={() => setShowBatchTagDialog(true)}
            className="rounded-lg p-1.5 text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)] transition-colors"
            title="添加标签"
          >
            <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M9.568 3H5.25A2.25 2.25 0 0 0 3 5.25v4.318c0 .597.237 1.17.659 1.591l9.581 9.581c.699.699 1.78.872 2.607.33a18.095 18.095 0 0 0 5.223-5.223c.542-.827.369-1.908-.33-2.607L11.16 3.66A2.25 2.25 0 0 0 9.568 3Z" />
              <path strokeLinecap="round" strokeLinejoin="round" d="M6 6h.008v.008H6V6Z" />
            </svg>
          </button>
          <button
            onClick={handleBatchDelete}
            className="rounded-lg p-1.5 text-[var(--color-text-muted)] hover:bg-[var(--color-danger-soft)] hover:text-[var(--color-danger)] transition-colors"
            title="删除"
          >
            <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
              <path strokeLinecap="round" strokeLinejoin="round" d="m14.74 9-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 0 1-2.244 2.077H8.084a2.25 2.25 0 0 1-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 0 0-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 0 1 3.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 0 0-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 0 0-7.5 0" />
            </svg>
          </button>
        </div>
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
          totalCount={displayMedia.length}
          onClose={() => setShowExportDialog(false)}
        />
      )}

      {/* Context menu */}
      {ctxMenu && (
        <div
          className="fixed z-[60] min-w-[150px] rounded-xl border border-[var(--color-border)] bg-[var(--color-bg-elevated)]/95 backdrop-blur-xl py-1.5 shadow-2xl shadow-black/30 animate-scale-in"
          style={{ left: ctxMenu.x, top: ctxMenu.y }}
          onClick={(e) => e.stopPropagation()}
        >
          {/* Single-item actions */}
          {!selectedIds.has(ctxMenu.media.id) || selectedIds.size <= 1 ? (
            <>
              <button
                onClick={() => {
                  const idx = displayMedia.findIndex((m) => m.id === ctxMenu.media.id);
                  if (idx >= 0) setLightboxIndex(idx);
                  setCtxMenu(null);
                }}
                className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-[var(--color-text-secondary)] transition-colors hover:bg-[var(--color-bg-hover)]"
              >
                <svg className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="M2.036 12.322a1.012 1.012 0 0 1 0-.639C3.423 7.51 7.36 4.5 12 4.5c4.638 0 8.573 3.007 9.963 7.178.07.207.07.431 0 .639C20.577 16.49 16.64 19.5 12 19.5c-4.638 0-8.573-3.007-9.963-7.178Z" />
                  <path strokeLinecap="round" strokeLinejoin="round" d="M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z" />
                </svg>
                查看原图
              </button>
              <button
                onClick={() => { setSelected(ctxMenu.media); setCtxMenu(null); }}
                className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-[var(--color-text-secondary)] transition-colors hover:bg-[var(--color-bg-hover)]"
              >
                <svg className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="M11.25 11.25l.041-.02a.75.75 0 011.063.852l-.708 2.836a.75.75 0 001.063.853l.041-.021M21 12a9 9 0 11-18 0 9 9 0 0118 0zm-9-3.75h.008v.008H12V8.25z" />
                </svg>
                查看详情
              </button>
              <div className="my-1 border-t border-[var(--color-border)]" />
            </>
          ) : (
            <>
              <div className="px-3 py-1.5 text-[11px] text-[var(--color-text-muted)]">
                已选中 {selectedIds.size} 张图片
              </div>
              <div className="my-1 border-t border-[var(--color-border)]" />
            </>
          )}
          <button
            onClick={() => {
              setShowBatchTagDialog(true);
              if (!selectedIds.has(ctxMenu.media.id)) setSelectedIds(new Set([ctxMenu.media.id]));
              setCtxMenu(null);
            }}
            className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-[var(--color-text-secondary)] transition-colors hover:bg-[var(--color-bg-hover)]"
          >
            <svg className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M9.568 3H5.25A2.25 2.25 0 0 0 3 5.25v4.318c0 .597.237 1.17.659 1.591l9.581 9.581c.699.699 1.78.872 2.607.33a18.095 18.095 0 0 0 5.223-5.223c.542-.827.369-1.908-.33-2.607L11.16 3.66A2.25 2.25 0 0 0 9.568 3Z" />
              <path strokeLinecap="round" strokeLinejoin="round" d="M6 6h.008v.008H6V6Z" />
            </svg>
            添加标签
            {selectedIds.has(ctxMenu.media.id) && selectedIds.size > 1 ? `（${selectedIds.size} 张）` : ""}
          </button>
          <button
            onClick={async () => {
              const ids = selectedIds.has(ctxMenu.media.id) ? Array.from(selectedIds) : [ctxMenu.media.id];
              const all = await loadCollections();
              setCollectionsForPicker(all);
              setAddToCollectionMediaIds(ids);
              setCollectionPickerSearch("");
              setShowAddToCollection(true);
              setCtxMenu(null);
            }}
            className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-[var(--color-text-secondary)] transition-colors hover:bg-[var(--color-bg-hover)]"
          >
            <svg className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M12 10.5v6m3-3H9m4.06-7.19-2.12-2.12a1.5 1.5 0 0 0-1.061-.44H4.5A2.25 2.25 0 0 0 2.25 6v12a2.25 2.25 0 0 0 2.25 2.25h15A2.25 2.25 0 0 0 21.75 18V9a2.25 2.25 0 0 0-2.25-2.25h-5.379a1.5 1.5 0 0 1-1.06-.44Z" />
            </svg>
            添加到集合
            {selectedIds.has(ctxMenu.media.id) && selectedIds.size > 1 ? `（${selectedIds.size} 张）` : ""}
          </button>
          {collectionId && (
            <button
              onClick={async () => {
                await removeFromCollection(collectionId, ctxMenu.media.id);
                setCtxMenu(null);
                loadMedia();
                window.dispatchEvent(new CustomEvent("collections-changed"));
              }}
              className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-orange-400 transition-colors hover:bg-orange-900/20"
            >
              从集合移除
            </button>
          )}
          <div className="my-1 border-t border-[var(--color-border)]" />
          {selectedIds.has(ctxMenu.media.id) && selectedIds.size > 1 ? (
            <>
              <button
                onClick={() => { setSelectedIds(new Set()); setCtxMenu(null); }}
                className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-[var(--color-text-secondary)] transition-colors hover:bg-[var(--color-bg-hover)]"
              >
                取消选中
              </button>
              <button
                onClick={() => {
                  setDeleteConfirm("batch");
                  setCtxMenu(null);
                }}
                className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-[var(--color-danger)] transition-colors hover:bg-[var(--color-danger-soft)]"
              >
                <svg className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="m14.74 9-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 0 1-2.244 2.077H8.084a2.25 2.25 0 0 1-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 0 0-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 0 1 3.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 0 0-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 0 0-7.5 0" />
                </svg>
                删除 {selectedIds.size} 张图片
              </button>
            </>
          ) : (
            <button
              onClick={() => {
                setPendingDeleteId(ctxMenu.media.id);
                setCtxMenu(null);
                setDeleteConfirm("single");
              }}
              className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-[var(--color-danger)] transition-colors hover:bg-[var(--color-danger-soft)]"
            >
              <svg className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                <path strokeLinecap="round" strokeLinejoin="round" d="m14.74 9-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 0 1-2.244 2.077H8.084a2.25 2.25 0 0 1-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 0 0-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 0 1 3.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 0 0-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 0 0-7.5 0" />
              </svg>
              删除
            </button>
          )}
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
          media={displayMedia}
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
