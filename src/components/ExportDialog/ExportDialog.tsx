import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { exportDataset } from "@/lib/tauri";
import type { ExportProgress } from "@/types/export";

interface ExportDialogProps {
  mediaIds: string[];
  totalCount: number;
  onClose: () => void;
}

const VARIANT_PRESETS = [
  { name: "web_share", label: "Web分享 (JPEG 1080px)" },
  { name: "print", label: "打印 (PNG 2048px)" },
  { name: "dataset", label: "训练数据集 (JPEG 512px)" },
];

function ExportDialog({ mediaIds, totalCount, onClose }: ExportDialogProps) {
  const [scope, setScope] = useState<"selected" | "current" | "all">("selected");
  const [captionMode, setCaptionMode] = useState<"all" | "manual" | "ai">("all");
  const [exportOriginal, setExportOriginal] = useState(true);
  const [variantPresets, setVariantPresets] = useState<string[]>([]);
  const [useZip, setUseZip] = useState(false);
  const [outputDir, setOutputDir] = useState("");
  const [exporting, setExporting] = useState(false);
  const [progress, setProgress] = useState<ExportProgress | null>(null);
  const [result, setResult] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const unlisten = listen<ExportProgress>("export-progress", (event) => {
      setProgress(event.payload);
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  const togglePreset = (name: string) => {
    setVariantPresets((prev) =>
      prev.includes(name) ? prev.filter((p) => p !== name) : [...prev, name]
    );
  };

  const handleExport = async () => {
    if (!outputDir.trim()) return;
    setExporting(true);
    setError(null);
    setResult(null);
    setProgress(null);

    try {
      const ids = scope === "selected" ? mediaIds : [];
      const path = await exportDataset({
        media_ids: ids,
        caption_mode: captionMode,
        export_original: exportOriginal,
        variant_presets: variantPresets,
        output_dir: outputDir,
        use_zip: useZip,
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
                    className="h-2 rounded-full bg-blue-500 transition-all"
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
                  当前筛选结果 ({totalCount})
                </label>
              </div>
            </div>

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
                <option value="all">全部</option>
                <option value="manual">仅手动</option>
                <option value="ai">仅 AI</option>
              </select>
            </div>

            {/* Variants */}
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
                {VARIANT_PRESETS.map((p) => (
                  <label
                    key={p.name}
                    className="flex items-center gap-2 text-xs text-[var(--color-text-secondary)]"
                  >
                    <input
                      type="checkbox"
                      checked={variantPresets.includes(p.name)}
                      onChange={() => togglePreset(p.name)}
                    />
                    {p.label}
                  </label>
                ))}
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

            {/* Output directory */}
            <div className="mb-4">
              <label className="mb-1 block text-xs text-[var(--color-text-muted)]">
                输出目录
              </label>
              <input
                type="text"
                value={outputDir}
                onChange={(e) => setOutputDir(e.target.value)}
                placeholder="C:\Users\...\export"
                className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)]"
              />
            </div>

            {/* Summary */}
            <p className="mb-4 text-[10px] text-[var(--color-text-muted)]">
              将导出 {scopeIds} 张图片{captionMode !== "all" ? ` (${captionMode === "manual" ? "仅手动" : "仅AI"} caption)` : ""}
              {variantPresets.length > 0 ? ` + ${variantPresets.length} 种变体` : ""}
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
                className="rounded bg-blue-600 px-4 py-1.5 text-xs font-medium text-white hover:bg-blue-500 disabled:opacity-50"
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
