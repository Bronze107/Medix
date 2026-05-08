import { useCallback, useEffect, useState } from "react";
import type { Media } from "@/types/media";
import { mediaImport, mediaList } from "@/lib/tauri";
import DropZone from "@/components/DropZone/DropZone";
import Gallery from "@/components/Gallery/Gallery";
import DetailPanel from "@/components/DetailPanel/DetailPanel";

type SortField = "imported_at" | "created_at" | "modified_at";

function AllMedia() {
  const [media, setMedia] = useState<Media[]>([]);
  const [selected, setSelected] = useState<Media | null>(null);
  const [sortBy, setSortBy] = useState<SortField>("imported_at");
  const [descending, setDescending] = useState(true);
  const [isImporting, setIsImporting] = useState(false);
  const [importMessage, setImportMessage] = useState("");

  const loadMedia = useCallback(async () => {
    try {
      const list = await mediaList(sortBy, descending);
      setMedia(list);
    } catch (e) {
      console.error("Failed to load media:", e);
    }
  }, [sortBy, descending]);

  useEffect(() => {
    loadMedia();
  }, [loadMedia]);

  const handleDropFiles = useCallback(
    async (files: File[]) => {
      const paths: string[] = [];
      for (const file of files) {
        const path = (file as unknown as { path?: string }).path;
        if (path) {
          paths.push(path);
        }
      }

      if (paths.length === 0) {
        setImportMessage("无法获取文件路径，请重试");
        setTimeout(() => setImportMessage(""), 3000);
        return;
      }

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
        setTimeout(() => setImportMessage(""), 5000);
      }
    },
    [loadMedia]
  );

  return (
    <div className="flex h-full flex-col">
      {/* Toolbar */}
      <div className="flex items-center justify-between border-b border-neutral-800 px-6 py-3">
        <h1 className="text-xl font-bold">全部媒体</h1>
        <div className="flex items-center gap-3">
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

      {/* Content */}
      <div className="flex flex-1 overflow-hidden">
        <div className="flex flex-1 flex-col p-4">
          {media.length === 0 ? (
            <DropZone onDropFiles={handleDropFiles} />
          ) : (
            <Gallery
              media={media}
              selectedId={selected?.id ?? null}
              onSelect={setSelected}
            />
          )}
        </div>
        <DetailPanel media={selected} />
      </div>
    </div>
  );
}

export default AllMedia;
