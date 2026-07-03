import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { showToast } from "@/components/Toast/Toast";
import type { Media } from "@/types/media";
import type { LineageGraph } from "@/types/lineage";
import { mediaGenerateDerivative, mediaImportDerivative, mediaLineageList } from "@/lib/tauri";

interface DerivativeDialogProps {
  media: Media;
  onClose: () => void;
  onDone: (lineage: LineageGraph) => void;
}

export default function DerivativeDialog({ media, onClose, onDone }: DerivativeDialogProps) {
  const [mode, setMode] = useState<"generate" | "import">("generate");
  const [format, setFormat] = useState("jpeg");
  const [filter, setFilter] = useState("triangle");
  const [maxWidth, setMaxWidth] = useState<number | undefined>();
  const [maxHeight, setMaxHeight] = useState<number | undefined>();
  const [preset, setPreset] = useState<string | null>(null);
  const [quality, setQuality] = useState(85);
  const [importPaths, setImportPaths] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);

  const presets = [
    { label: "1/2", w: media.width ? Math.round(media.width / 2) : undefined, h: media.height ? Math.round(media.height / 2) : undefined },
    { label: "1/4", w: media.width ? Math.round(media.width / 4) : undefined, h: media.height ? Math.round(media.height / 4) : undefined },
    { label: "256", w: 256, h: 256 },
  ];

  const handleGenerate = async () => {
    if (busy) return;
    setBusy(true);
    try {
      const result = await mediaGenerateDerivative(
        media.id, "", format, maxWidth, maxHeight, quality, filter || null,
      );
      showToast(`衍生图已生成: ${result.id.slice(0, 8)}...`);
      const g = await mediaLineageList(media.id);
      onDone(g);
      window.dispatchEvent(new Event("derivative-changed"));
      onClose();
    } catch (e) {
      showToast("生成失败: " + (e as Error).message);
    } finally {
      setBusy(false);
    }
  };

  const handleImport = async () => {
    if (busy || importPaths.length === 0) return;
    setBusy(true);
    try {
      for (const fp of importPaths) {
        const result = await mediaImportDerivative(media.id, fp);
        showToast(`已导入: ${result.id.slice(0, 8)}...`);
      }
      const g = await mediaLineageList(media.id);
      onDone(g);
      window.dispatchEvent(new Event("derivative-changed"));
      onClose();
    } catch (e) {
      showToast("导入失败: " + (e as Error).message);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-[var(--color-bg-overlay)] animate-fade-in"
      onClick={onClose}
    >
      <div
        className="w-80 rounded-xl border border-[var(--color-border)] bg-[var(--color-bg-elevated)] p-5 shadow-2xl animate-scale-in"
        onClick={(e) => e.stopPropagation()}
      >
        <h3 className="mb-4 text-sm font-semibold text-[var(--color-text-primary)]">
          衍生图
        </h3>

        {/* Mode toggle */}
        <div className="mb-4 flex gap-1">
          <button
            onClick={() => setMode("generate")}
            className={`flex-1 rounded px-3 py-1.5 text-xs font-medium transition-colors ${
              mode === "generate"
                ? "bg-[var(--color-accent-soft)] text-[var(--color-accent)]"
                : "text-[var(--color-text-muted)] hover:text-[var(--color-text-secondary)]"
            }`}
          >
            生成
          </button>
          <button
            onClick={() => setMode("import")}
            className={`flex-1 rounded px-3 py-1.5 text-xs font-medium transition-colors ${
              mode === "import"
                ? "bg-[var(--color-accent-soft)] text-[var(--color-accent)]"
                : "text-[var(--color-text-muted)] hover:text-[var(--color-text-secondary)]"
            }`}
          >
            导入
          </button>
        </div>

        {mode === "generate" ? (
          <div className="space-y-3">
            {/* Size presets */}
            <div>
              <label className="mb-1.5 block text-xs text-[var(--color-text-muted)]">尺寸</label>
              <div className="flex gap-1">
                {presets.map((p) => (
                  <button
                    key={p.label}
                    onClick={() => {
                      setMaxWidth(p.w);
                      setMaxHeight(p.h);
                      setPreset(p.label);
                    }}
                    className={`rounded px-2.5 py-1 text-xs transition-colors ${
                      preset === p.label
                        ? "bg-[var(--color-accent-soft)] text-[var(--color-accent)]"
                        : "bg-[var(--color-bg-tertiary)] text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
                    }`}
                  >
                    {p.label}
                  </button>
                ))}
                <button
                  onClick={() => {
                    setMaxWidth(undefined);
                    setMaxHeight(undefined);
                    setPreset("custom");
                  }}
                  className={`rounded px-2.5 py-1 text-xs transition-colors ${
                    preset === "custom"
                      ? "bg-[var(--color-accent-soft)] text-[var(--color-accent)]"
                      : "bg-[var(--color-bg-tertiary)] text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
                  }`}
                >
                  自定义
                </button>
              </div>
            </div>

            {/* Format + Filter */}
            <div className="flex gap-3">
              <div className="flex-1">
                <label className="mb-1 block text-xs text-[var(--color-text-muted)]">格式</label>
                <select
                  value={format}
                  onChange={(e) => setFormat(e.target.value)}
                  className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none focus:border-[var(--color-accent)]"
                >
                  <option value="jpeg">JPEG</option>
                  <option value="png">PNG</option>
                </select>
              </div>
              <div className="flex-1">
                <label className="mb-1 block text-xs text-[var(--color-text-muted)]">滤镜</label>
                <select
                  value={filter}
                  onChange={(e) => setFilter(e.target.value)}
                  className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none focus:border-[var(--color-accent)]"
                >
                  <option value="triangle">Triangle</option>
                  <option value="nearest">Nearest</option>
                  <option value="catmullrom">CatmullRom</option>
                  <option value="gaussian">Gaussian</option>
                  <option value="lanczos3">Lanczos3</option>
                </select>
              </div>
            </div>

            {/* Custom dimensions */}
            <div className="flex gap-3">
              <div className="flex-1">
                <label className="mb-1 block text-xs text-[var(--color-text-muted)]">最大宽度</label>
                <input
                  type="number"
                  value={maxWidth ?? ""}
                  onChange={(e) => {
                    setMaxWidth(e.target.value ? Number(e.target.value) : undefined);
                    setPreset("custom");
                  }}
                  placeholder="自动"
                  min={1}
                  className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)] focus:border-[var(--color-accent)]"
                />
              </div>
              <div className="flex-1">
                <label className="mb-1 block text-xs text-[var(--color-text-muted)]">最大高度</label>
                <input
                  type="number"
                  value={maxHeight ?? ""}
                  onChange={(e) => {
                    setMaxHeight(e.target.value ? Number(e.target.value) : undefined);
                    setPreset("custom");
                  }}
                  placeholder="自动"
                  min={1}
                  className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)] focus:border-[var(--color-accent)]"
                />
              </div>
            </div>

            {/* Quality (JPEG only) */}
            {format === "jpeg" && (
              <div>
                <label className="mb-1 block text-xs text-[var(--color-text-muted)]">
                  质量: {quality}
                </label>
                <input
                  type="range"
                  min={1}
                  max={100}
                  value={quality}
                  onChange={(e) => setQuality(Number(e.target.value))}
                  className="w-full accent-[var(--color-accent)]"
                />
              </div>
            )}

            {/* Actions */}
            <div className="flex justify-end gap-2 pt-1">
              <button
                onClick={onClose}
                className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-3 py-1.5 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)] transition-colors"
              >
                取消
              </button>
              <button
                onClick={handleGenerate}
                disabled={busy}
                className="rounded bg-[var(--color-accent)] px-4 py-1.5 text-xs font-medium text-white hover:bg-[var(--color-accent-hover)] active:scale-[0.97] disabled:opacity-50 transition-all"
              >
                {busy ? "生成中..." : "生成"}
              </button>
            </div>
          </div>
        ) : (
          <div className="space-y-3">
            <button
              onClick={async () => {
                try {
                  const selected = await open({
                    multiple: true,
                    filters: [{
                      name: "图片",
                      extensions: ["jpg", "jpeg", "png", "webp", "gif", "bmp"],
                    }],
                  });
                  if (selected) {
                    setImportPaths(Array.isArray(selected) ? selected : [selected]);
                  }
                } catch (e) {
                  console.error("File picker error:", e);
                }
              }}
              className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-3 py-2 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)] active:scale-[0.97] transition-all"
            >
              选择文件...
            </button>

            {importPaths.length > 0 && (
              <div className="max-h-28 overflow-auto rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] p-2">
                {importPaths.map((p, i) => (
                  <div key={i} className="truncate text-[11px] text-[var(--color-text-muted)]">
                    {p.split(/[/\\]/).pop()}
                  </div>
                ))}
              </div>
            )}

            <div className="flex justify-end gap-2 pt-1">
              <button
                onClick={onClose}
                className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-3 py-1.5 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)] transition-colors"
              >
                取消
              </button>
              <button
                onClick={handleImport}
                disabled={importPaths.length === 0 || busy}
                className="rounded bg-[var(--color-accent)] px-4 py-1.5 text-xs font-medium text-white hover:bg-[var(--color-accent-hover)] active:scale-[0.97] disabled:opacity-50 transition-all"
              >
                {busy ? "导入中..." : `导入 (${importPaths.length})`}
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
