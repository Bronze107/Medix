import { useCallback, useEffect, useState } from "react";
import { useSearchParams } from "react-router";
import { listen } from "@tauri-apps/api/event";
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

      setIsImporting(true);
      setImportMessage(`正在导入 ${paths.length} 个文件...`);

      try {
        const results = await mediaImport(paths);
        const successCount = results.filter((r) => r.success).length;
        const failCount = results.length - successCount;
        setImportMessage(
          `导入完成: ${successCount} 成功${failCount > 0 ? `, ${failCount} 失败` : ""}`
        );
        await loadMedia();
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
    return () => {
      unlistenEnter.then((f) => f());
      unlistenLeave.then((f) => f());
      unlistenDrop.then((f) => f());
    };
  }, [doImport]);

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
      <div className="flex items-center justify-between border-b border-neutral-800 px-6 py-3">
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
            className="rounded border border-neutral-700 bg-neutral-800 px-2 py-1 text-xs text-neutral-300 outline-none"
          >
            <option value="imported_at">按导入时间</option>
            <option value="created_at">按创建时间</option>
            <option value="modified_at">按修改时间</option>
          </select>
          <button
            onClick={() => setDescending((d) => !d)}
            className="rounded border border-neutral-700 bg-neutral-800 px-2 py-1 text-xs text-neutral-300 hover:bg-neutral-700"
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
                : "border-neutral-700 bg-neutral-800 text-neutral-300 hover:bg-neutral-700"
            }`}
          >
            {selectionMode ? "退出选择" : "批量选择"}
          </button>
          {debouncedSearch && (
            <button
              onClick={() => setShowSaveDialog(true)}
              className="rounded border border-neutral-700 bg-neutral-800 px-2 py-1 text-xs text-neutral-300 hover:bg-neutral-700"
              title="保存当前筛选"
            >
              保存筛选
            </button>
          )}
          <span className="text-xs text-neutral-500">{media.length} 项</span>
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
        <div className="flex items-center justify-between border-b border-neutral-800 bg-neutral-800/80 px-6 py-2">
          <span className="text-sm text-neutral-300">
            已选择 <span className="font-bold text-green-400">{selectedIds.size}</span> 项
          </span>
          <div className="flex items-center gap-2">
            <button
              onClick={handleSelectAll}
              className="rounded border border-neutral-700 bg-neutral-800 px-2 py-1 text-xs text-neutral-300 hover:bg-neutral-700"
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
              onClick={() => setSelectedIds(new Set())}
              className="rounded border border-neutral-700 bg-neutral-800 px-2 py-1 text-xs text-neutral-300 hover:bg-neutral-700"
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
                className="mb-4 h-10 w-10 text-neutral-600"
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
              <p className="text-sm text-neutral-500">没有找到匹配的媒体</p>
              <p className="mt-1 text-xs text-neutral-600">
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
              selectedIds={Array.from(selectedIds)}
              selectionMode={selectionMode}
              onToggleSelect={handleToggleSelect}
            />
          )}
        </div>
        <DetailPanel media={selected} />
      </div>

      {/* Batch tag dialog */}
      {showBatchTagDialog && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
          onClick={() => setShowBatchTagDialog(false)}
        >
          <div
            className="w-80 rounded-lg border border-neutral-700 bg-neutral-800 p-4 shadow-xl"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 className="mb-3 text-sm font-bold text-neutral-200">
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
              className="mb-3 w-full rounded border border-neutral-700 bg-neutral-900 px-2 py-1.5 text-xs text-neutral-200 outline-none placeholder:text-neutral-500 focus:border-blue-500"
            />
            <div className="max-h-48 overflow-auto space-y-1">
              {filteredBatchTags.length === 0 && !batchTagSearch.trim() && (
                <p className="py-2 text-center text-xs text-neutral-500">暂无标签</p>
              )}
              {filteredBatchTags.map((tag) => (
                <button
                  key={tag.id}
                  onClick={() => handleBatchTagAdd(tag.id)}
                  className="block w-full rounded px-2 py-1.5 text-left text-xs text-neutral-300 hover:bg-neutral-700"
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
                className="rounded border border-neutral-700 bg-neutral-800 px-3 py-1 text-xs text-neutral-300 hover:bg-neutral-700"
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
            className="w-80 rounded-lg border border-neutral-700 bg-neutral-800 p-4 shadow-xl"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 className="mb-3 text-sm font-bold text-neutral-200">保存筛选器</h3>
            <p className="mb-1 text-[10px] text-neutral-500">查询条件</p>
            <p className="mb-3 rounded bg-neutral-900 px-2 py-1.5 text-xs text-neutral-300 font-mono">
              {debouncedSearch}
            </p>
            <label className="mb-1 block text-[10px] text-neutral-500">名称</label>
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
              className="mb-3 w-full rounded border border-neutral-700 bg-neutral-900 px-2 py-1.5 text-xs text-neutral-200 outline-none placeholder:text-neutral-500 focus:border-blue-500"
            />
            <div className="flex justify-end gap-2">
              <button
                onClick={() => {
                  setShowSaveDialog(false);
                  setSavedFilterName("");
                }}
                className="rounded border border-neutral-700 bg-neutral-800 px-3 py-1 text-xs text-neutral-300 hover:bg-neutral-700"
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
    </div>
  );
}

export default AllMedia;
