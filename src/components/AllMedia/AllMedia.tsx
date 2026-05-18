import { useCallback, useEffect, useState } from "react";
import { useSearchParams } from "react-router";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type { Media } from "@/types/media";
import type { Tag } from "@/types/tag";
import {
  mediaImport,
  mediaList,
  mediaSearch,
  mediaTagAddBatch,
  savedFiltersSave,
  tagCreate,
  tagList,
} from "@/lib/tauri";
import DropZone from "@/components/DropZone/DropZone";
import Gallery from "@/components/Gallery/Gallery";
import DetailPanel from "@/components/DetailPanel/DetailPanel";
import SearchBar from "@/components/SearchBar/SearchBar";
import ExportDialog from "@/components/ExportDialog/ExportDialog";
import Lightbox from "@/components/Lightbox/Lightbox";
import { aiPendingCount, mediaFindDuplicates, mediaSoftDelete } from "@/lib/tauri";
import { importZip } from "@/lib/tauri";

type SortField = "imported_at" | "created_at" | "modified_at";

interface DragDropPayload {
  paths: string[];
  position: { x: number; y: number };
}

function AllMedia() {
  const [searchParams] = useSearchParams();
  const initialQuery = searchParams.get("q") ?? "";

  const [media, setMedia] = useState<Media[]>([]);
  const [selected, setSelected] = useState<Media | null>(null);
  const [sortBy, setSortBy] = useState<SortField>("imported_at");
  const [descending, setDescending] = useState(true);
  const [isImporting, setIsImporting] = useState(false);
  const [importMessage, setImportMessage] = useState("");
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

  // Batch selection
  const [selectionMode, setSelectionMode] = useState(false);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());

  // Batch tag dialog
  const [showBatchTagDialog, setShowBatchTagDialog] = useState(false);
  const [batchTagSearch, setBatchTagSearch] = useState("");
  const [allTags, setAllTags] = useState<Tag[]>([]);

  // Debounce search input
  useEffect(() => {
    const timer = setTimeout(() => {
      setDebouncedSearch(searchQuery.trim());
    }, 300);
    return () => clearTimeout(timer);
  }, [searchQuery]);

  const loadMedia = useCallback(async () => {
    try {
      const query = debouncedSearch.trim();
      let list: Media[];
      if (query) {
        list = await mediaSearch(query, sortBy, descending);
      } else {
        list = await mediaList(sortBy, descending);
      }
      setMedia(list);
    } catch (e) {
      console.error("Failed to load media:", e);
    }
  }, [sortBy, descending, debouncedSearch]);

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

        // Import images
        if (images.length > 0) {
          setImportMessage(`正在导入 ${images.length} 个文件...`);
          const results = await mediaImport(images);
          totalImported += results.filter((r) => r.success && !r.error?.includes("重复")).length;
          totalDuplicates += results.filter((r) => r.success && r.error?.includes("重复")).length;
          totalFailed += results.filter((r) => !r.success).length;
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
        const pending = await aiPendingCount();
        setAiRemaining(pending);
      } catch (e) {
        setImportMessage(`导入失败: ${e}`);
      } finally {
        setIsImporting(false);
        setDropHover(false);
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

    return () => {
      unlistenEnter.then((f) => f());
      unlistenLeave.then((f) => f());
      unlistenDrop.then((f) => f());
      unlistenRemote.then((f) => f());
      unlistenAiDone.then((f) => f());
    };
  }, [doImport, loadMedia]);

  const handleToggleSelect = (item: Media) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(item.id)) {
        next.delete(item.id);
      } else {
        next.add(item.id);
      }
      return next;
    });
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

  const handleBatchDelete = async () => {
    if (selectedIds.size === 0) return;
    if (!confirm(`确定要删除 ${selectedIds.size} 张图片吗？\n可以在回收站中恢复。`)) return;
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
    loadMedia();
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
    <div className="flex h-full flex-col">
      {/* Toolbar */}
      <div className="flex items-center justify-between border-b border-[var(--color-border)] px-6 py-3">
        <h1 className="text-xl font-bold">全部媒体</h1>
        <div className="flex items-center gap-3">
          <SearchBar
            value={searchQuery}
            onChange={setSearchQuery}
            onClear={() => setSearchQuery("")}
          />
          <select
            value={sortBy}
            onChange={(e) => setSortBy(e.target.value as SortField)}
            className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1 text-xs text-[var(--color-text-secondary)] outline-none"
          >
            <option value="imported_at">按导入时间</option>
            <option value="created_at">按创建时间</option>
            <option value="modified_at">按修改时间</option>
          </select>
          <button
            onClick={() => setDescending((d) => !d)}
            className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
          >
            {descending ? "降序" : "升序"}
          </button>
          <button
            onClick={() => {
              setSelectionMode((m) => !m);
              setSelectedIds(new Set());
              if (selectionMode) setSelected(null);
            }}
            className={`rounded border px-2 py-1 text-xs transition-colors ${
              selectionMode
                ? "border-green-600 bg-green-900/30 text-green-400 hover:bg-green-900/50"
                : "border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
            }`}
          >
            {selectionMode ? "退出选择" : "批量选择"}
          </button>
          <button
            onClick={() => setShowExportDialog(true)}
            disabled={media.length === 0}
            className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)] disabled:opacity-50"
          >
            导出
          </button>
          <button
            onClick={async () => {
              setFindingDups(true);
              try {
                const groups = await mediaFindDuplicates();
                setDupGroups(groups);
                setShowDupDialog(true);
              } catch (e) {
                console.error("Failed to find duplicates:", e);
              } finally {
                setFindingDups(false);
              }
            }}
            disabled={findingDups}
            className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)] disabled:opacity-50"
          >
            {findingDups ? "分析中..." : "查找重复"}
          </button>
          <button
            onClick={() => setShowImportDialog(true)}
            className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
          >
            导入ZIP
          </button>
          {debouncedSearch && (
            <button
              onClick={() => setShowSaveDialog(true)}
              className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
              title="保存当前筛选"
            >
              保存筛选
            </button>
          )}
          <span className="text-xs text-[var(--color-text-muted)]">{media.length} 项</span>
          {aiRemaining > 0 && (
            <span className="rounded-full bg-blue-900/30 px-2 py-0.5 text-[11px] text-blue-400">
              AI 处理中 · {aiRemaining} 剩余
            </span>
          )}
        </div>
      </div>

      {/* Import message */}
      {importMessage && (
        <div
          className={`px-6 py-2 text-xs ${
            isImporting
              ? "bg-blue-900/30 text-blue-400"
              : importMessage.includes("失败")
              ? "bg-red-900/30 text-red-400"
              : "bg-green-900/30 text-green-400"
          }`}
        >
          {importMessage}
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
              onClick={() => setShowBatchTagDialog(true)}
              className="rounded bg-blue-600 px-3 py-1 text-xs font-medium text-white hover:bg-blue-500"
            >
              添加标签
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
                  onClick={() => setSearchQuery("")}
                  className="ml-1 text-blue-400 hover:text-blue-300"
                >
                  重置搜索
                </button>
              </p>
            </div>
          ) : (
            <Gallery
              media={media}
              selectedId={selected?.id ?? null}
              onSelect={setSelected}
              onDoubleClick={(item) => {
                const idx = media.findIndex((m) => m.id === item.id);
                if (idx >= 0) setLightboxIndex(idx);
              }}
              selectedIds={Array.from(selectedIds)}
              selectionMode={selectionMode}
              onToggleSelect={handleToggleSelect}
            />
          )}
        </div>
        <DetailPanel media={selected} onDeleted={() => { setSelected(null); loadMedia(); }} />
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
              className="mb-3 w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)] focus:border-blue-500"
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
                  className="block w-full rounded bg-blue-600/20 px-2 py-1.5 text-left text-xs text-blue-400 hover:bg-blue-600/30"
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
                  setShowSaveDialog(false);
                  setSavedFilterName("");
                }
              }}
              placeholder="输入筛选器名称..."
              autoFocus
              className="mb-3 w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)] focus:border-blue-500"
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
                  setShowSaveDialog(false);
                  setSavedFilterName("");
                }}
                disabled={!savedFilterName.trim()}
                className="rounded bg-blue-600 px-3 py-1 text-xs font-medium text-white hover:bg-blue-500 disabled:opacity-50"
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
                    className="rounded bg-blue-600 px-4 py-1.5 text-xs font-medium text-white hover:bg-blue-500 disabled:opacity-50"
                  >
                    {importing ? "导入中..." : "开始导入"}
                  </button>
                </div>
              </>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

export default AllMedia;
