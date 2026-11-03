import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import {
  datasetItemOverridesList,
  datasetRecipeGet,
  exportDataset,
} from "@/lib/tauri";
import type { ExportProgress } from "@/types/export";
import type { DatasetRecipe } from "@/types/dataset";

interface ExportDialogProps {
  mediaIds: string[];
  /** Whether the selection includes any originals. */
  hasOriginals?: boolean;
  totalCount: number;
  /** 数据集模式：传入集合 id 时显示配方选项并按集合展开。 */
  collectionId?: string;
  /** 「当前筛选结果」范围使用的搜索查询。 */
  searchQuery?: string;
  onClose: () => void;
}

const EXPORT_PREFS_KEY = "medix.exportPrefs";
interface ExportPrefs {
  captionMode: "all" | "manual" | "ai" | "latest";
  exportJSON: boolean;
  useZip: boolean;
  lastDir: string;
}
function loadPrefs(): ExportPrefs {
  try {
    const raw = localStorage.getItem(EXPORT_PREFS_KEY);
    if (raw) return JSON.parse(raw);
  } catch {}
  return { captionMode: "all", exportJSON: true, useZip: false, lastDir: "" };
}

function ExportDialog({
  mediaIds,
  totalCount,
  collectionId,
  searchQuery,
  onClose,
}: ExportDialogProps) {
  const saved = loadPrefs();
  // 无选中项时「选中项」radio 不会渲染，scope 不能停在不可见的 "selected" 上。
  const initialScope: "selected" | "current" = mediaIds.length > 0 ? "selected" : "current";
  const [scope, setScope] = useState<"selected" | "current" | "all">(initialScope);
  const [captionMode, setCaptionMode] = useState<ExportPrefs["captionMode"] | "recipe">(saved.captionMode);
  // 原图默认不勾选：配方模式下图片按规则生成、无规则条目始终导出原图，
  // 这个开关只影响旧路径（未应用配方的选中项/筛选结果导出）。
  const [exportOriginal, setExportOriginal] = useState(false);
  const [exportJSON, setExportJSON] = useState(saved.exportJSON);
  const [useZip, setUseZip] = useState(saved.useZip);
  const [outputDir, setOutputDir] = useState(saved.lastDir);
  const [exporting, setExporting] = useState(false);
  const [progress, setProgress] = useState<ExportProgress | null>(null);
  const [result, setResult] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  // 数据集模式
  const [recipe, setRecipe] = useState<DatasetRecipe | null>(null);
  const [overrideCount, setOverrideCount] = useState(0);
  const [useRecipe, setUseRecipe] = useState(true);

  useEffect(() => {
    const unlisten = listen<ExportProgress>("export-progress", (event) => {
      setProgress(event.payload);
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  // 数据集模式：加载配方摘要 + 覆盖计数
  useEffect(() => {
    if (!collectionId) return;
    let cancelled = false;
    Promise.all([datasetRecipeGet(collectionId), datasetItemOverridesList(collectionId)])
      .then(([r, overrides]) => {
        if (cancelled) return;
        setRecipe(r);
        setUseRecipe(r !== null);
        setOverrideCount(overrides.length);
        // 数据集上下文里有配方时，caption 默认跟随配方
        if (r) setCaptionMode("recipe");
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [collectionId]);

  useEffect(() => {
    if (!collectionId) return;
    const unlisten = listen("dataset-recipe-changed", () => {
      datasetRecipeGet(collectionId)
        .then((r) => setRecipe(r))
        .catch(() => {});
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, [collectionId]);

  const handleExport = async () => {
    if (!outputDir.trim()) return;
    // Persist preferences
    localStorage.setItem(EXPORT_PREFS_KEY, JSON.stringify({
      captionMode: captionMode === "recipe" ? "all" : captionMode,
      exportJSON, useZip, lastDir: outputDir,
    }));
    setExporting(true);
    setError(null);
    setResult(null);
    setProgress(null);

    try {
      const options =
        scope === "selected"
          ? { media_ids: mediaIds }
          : collectionId
            ? { media_ids: [], collection_id: collectionId, use_recipe: useRecipe }
            : { media_ids: [], query: searchQuery ?? "" };
      const path = await exportDataset({
        caption_mode: captionMode,
        export_original: exportOriginal,
        export_json: exportJSON,
        output_dir: outputDir,
        use_zip: useZip,
        ...options,
      });
      setResult(path);
    } catch (e) {
      setError(String(e));
    } finally {
      setExporting(false);
    }
  };

  const scopeIds = scope === "selected" ? mediaIds.length : totalCount;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
      onClick={onClose}
    >
      <div
        className="w-96 rounded-lg border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] p-5 shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <h3 className="mb-4 text-sm font-bold text-[var(--color-text-primary)]">
          导出数据集
        </h3>

        {exporting || result || error ? (
          <div className="space-y-3">
            {progress && (
              <>
                <div className="h-2 w-full rounded-full bg-[var(--color-bg-secondary)]">
                  <div
                    className="h-2 rounded-full bg-[var(--color-accent)] transition-all"
                    style={{
                      width: `${(progress.current / progress.total) * 100}%`,
                    }}
                  />
                </div>
                <p className="text-center text-xs text-[var(--color-text-muted)]">
                  {progress.current} / {progress.total}
                </p>
              </>
            )}
            {result && (
              <div className="rounded bg-green-900/20 p-3">
                <p className="text-xs text-green-400">导出完成</p>
                <p className="mt-1 break-all font-mono text-[10px] text-[var(--color-text-muted)]">
                  {result}
                </p>
              </div>
            )}
            {error && (
              <div className="rounded bg-red-900/20 p-3">
                <p className="text-xs text-red-400">导出失败: {error}</p>
              </div>
            )}
            <button
              onClick={onClose}
              className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] px-3 py-1.5 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
            >
              关闭
            </button>
          </div>
        ) : (
          <>
            {/* Scope */}
            <div className="mb-4">
              <label className="mb-2 block text-xs text-[var(--color-text-muted)]">
                导出范围
              </label>
              <div className="space-y-1">
                {mediaIds.length > 0 && (
                  <label className="flex items-center gap-2 text-xs text-[var(--color-text-secondary)]">
                    <input
                      type="radio"
                      name="scope"
                      checked={scope === "selected"}
                      onChange={() => setScope("selected")}
                    />
                    选中项 ({mediaIds.length})
                  </label>
                )}
                <label className="flex items-center gap-2 text-xs text-[var(--color-text-secondary)]">
                  <input
                    type="radio"
                    name="scope"
                    checked={scope === "current"}
                    onChange={() => setScope("current")}
                  />
                  {collectionId ? "整个数据集" : "当前筛选结果"} ({totalCount})
                </label>
              </div>
            </div>

            {/* Dataset recipe */}
            {collectionId && (
              <div className="mb-4 rounded-lg border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] p-2.5">
                {recipe ? (
                  <>
                    <label className="flex items-center gap-2 text-xs text-[var(--color-text-secondary)]">
                      <input
                        type="checkbox"
                        checked={useRecipe}
                        onChange={(e) => setUseRecipe(e.target.checked)}
                      />
                      应用导出配方（{recipe.rules.length} 条规格规则
                      {recipe.naming_template !== "{stem}" ? ` · ${recipe.naming_template}` : ""}）
                    </label>
                    {overrideCount > 0 && (
                      <p className="mt-1 pl-6 text-[11px] text-[var(--color-text-muted)]">
                        {overrideCount} 个条目有单独覆盖设置
                      </p>
                    )}
                    <p className="mt-1 pl-6 text-[11px] text-[var(--color-text-muted)]">
                      配方生效时图片按规则生成；视频等无规则条目始终导出原图
                    </p>
                  </>
                ) : (
                  <p className="text-[11px] text-[var(--color-text-muted)]">
                    此数据集未设置导出配方，将按原图导出
                  </p>
                )}
              </div>
            )}

            {/* Caption mode */}
            <div className="mb-4">
              <label className="mb-2 block text-xs text-[var(--color-text-muted)]">
                Caption
              </label>
              <select
                value={captionMode}
                onChange={(e) => setCaptionMode(e.target.value as typeof captionMode)}
                className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none"
              >
                {collectionId && <option value="recipe">跟随配方</option>}
                <option value="all">全部</option>
                <option value="latest">最新一条</option>
                <option value="manual">仅手动</option>
                <option value="ai">仅 AI</option>
              </select>
            </div>

            {/* Options */}
            <div className="mb-4">
              <label className="mb-2 block text-xs text-[var(--color-text-muted)]">
                包含
              </label>
              <div className="space-y-1">
                <label className="flex items-center gap-2 text-xs text-[var(--color-text-secondary)]">
                  <input
                    type="checkbox"
                    checked={exportOriginal}
                    onChange={(e) => setExportOriginal(e.target.checked)}
                  />
                  原图
                </label>
                <label className="flex items-center gap-2 text-xs text-[var(--color-text-secondary)]">
                  <input
                    type="checkbox"
                    checked={exportJSON}
                    onChange={(e) => setExportJSON(e.target.checked)}
                  />
                  JSON 元数据
                </label>
              </div>
            </div>

            {/* ZIP / folder */}
            <div className="mb-4">
              <label className="mb-2 block text-xs text-[var(--color-text-muted)]">
                导出模式
              </label>
              <div className="space-y-1">
                <label className="flex items-center gap-2 text-xs text-[var(--color-text-secondary)]">
                  <input
                    type="radio"
                    name="mode"
                    checked={!useZip}
                    onChange={() => setUseZip(false)}
                  />
                  复制到目录
                </label>
                <label className="flex items-center gap-2 text-xs text-[var(--color-text-secondary)]">
                  <input
                    type="radio"
                    name="mode"
                    checked={useZip}
                    onChange={() => setUseZip(true)}
                  />
                  ZIP 打包
                </label>
              </div>
            </div>

            {/* Output path */}
            <div className="mb-4">
              <label className="mb-1 block text-xs text-[var(--color-text-muted)]">
                输出路径
              </label>
              <div className="flex gap-1.5">
                <input
                  type="text"
                  value={outputDir}
                  onChange={(e) => setOutputDir(e.target.value)}
                  placeholder={useZip ? "C:\\Users\\...\\export.zip" : "C:\\Users\\...\\export"}
                  className="flex-1 rounded border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)]"
                />
                <button
                  onClick={async () => {
                    if (useZip) {
                      const selected = await open({
                        multiple: false,
                        filters: [{ name: "ZIP", extensions: ["zip"] }],
                      });
                      if (selected) setOutputDir(selected);
                    } else {
                      const selected = await open({ directory: true });
                      if (selected) setOutputDir(selected);
                    }
                  }}
                  className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] px-2 py-1.5 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
                  title={useZip ? "选择 ZIP 文件" : "选择目录"}
                >
                  ...
                </button>
              </div>
            </div>

            {/* Summary */}
            <p className="mb-4 text-[11px] text-[var(--color-text-muted)]">
              将导出 {scopeIds} 个条目
              {captionMode !== "all"
                ? ` (${captionMode === "manual" ? "仅手动" : captionMode === "ai" ? "仅AI" : captionMode === "recipe" ? "跟随配方" : "仅最新 caption"})`
                : ""}
              {scope !== "selected" && collectionId && useRecipe && recipe
                ? " · 按配方生成规格产物"
                : ""}
              {useZip ? " → ZIP" : " → 目录"}
            </p>

            {/* Actions */}
            <div className="flex justify-end gap-2">
              <button
                onClick={onClose}
                className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] px-3 py-1.5 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
              >
                取消
              </button>
              <button
                onClick={handleExport}
                disabled={!outputDir.trim()}
                className="rounded bg-[var(--color-accent)] px-4 py-1.5 text-xs font-medium text-white hover:bg-[var(--color-accent-hover)] disabled:opacity-50"
              >
                开始导出
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

export default ExportDialog;
