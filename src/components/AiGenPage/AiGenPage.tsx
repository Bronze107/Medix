import { useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { imageGenerate, imageConfirmImport, imageDiscardStaged } from "@/lib/tauri";
import type { StagedImage } from "@/lib/tauri";

const ASPECT_RATIOS = ["auto", "1:1", "4:3", "3:4", "16:9", "9:16", "2:3", "3:2", "1:2", "2:1"];
const RESOLUTIONS = ["1k", "2k"];

function AiGenPage() {
  const [prompt, setPrompt] = useState("");
  const [aspectRatio, setAspectRatio] = useState("auto");
  const [resolution, setResolution] = useState("1k");
  const [n, setN] = useState(1);
  const [generating, setGenerating] = useState(false);
  const [staged, setStaged] = useState<StagedImage[]>([]);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [importing, setImporting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState<string | null>(null);
  const previewRef = useRef<HTMLDivElement>(null);

  // Default select all staged images
  useEffect(() => {
    if (staged.length > 0) {
      setSelectedIds(new Set(staged.map((s) => s.id)));
    }
  }, [staged]);

  const toggleSelect = (id: string) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id); else next.add(id);
      return next;
    });
  };

  const selectAll = () => setSelectedIds(new Set(staged.map((s) => s.id)));
  const deselectAll = () => setSelectedIds(new Set());

  const handleGenerate = async () => {
    if (!prompt.trim()) return;
    setGenerating(true);
    setError(null);
    try {
      const results = await imageGenerate(prompt.trim(), aspectRatio, resolution, n);
      setStaged(results);
      if (results.length === 0) setError("未生成任何图片，请尝试修改提示词。");
    } catch (e) {
      setError(String(e));
    } finally {
      setGenerating(false);
    }
  };

  const handleImport = async () => {
    const toImport = Array.from(selectedIds);
    if (toImport.length === 0) return;
    setImporting(true);
    try {
      await imageConfirmImport(toImport, prompt.trim(), null);
      // Remove imported from staged list
      const importedSet = new Set(toImport);
      setStaged((prev) => prev.filter((s) => !importedSet.has(s.id)));
      setSelectedIds(new Set());
    } catch (e) {
      setError(String(e));
    } finally {
      setImporting(false);
    }
  };

  const handleDiscard = async () => {
    const toDiscard = staged.filter((s) => !selectedIds.has(s.id)).map((s) => s.id);
    if (toDiscard.length === 0) {
      // Discard all if everything is selected
      const allIds = staged.map((s) => s.id);
      await imageDiscardStaged(allIds);
      setStaged([]);
      setSelectedIds(new Set());
      return;
    }
    await imageDiscardStaged(toDiscard);
    setStaged((prev) => prev.filter((s) => selectedIds.has(s.id)));
  };

  const selected = Array.from(selectedIds);

  return (
    <>
    <div className="flex h-full flex-col p-6">
      <h1 className="mb-6 text-xl font-semibold text-[var(--color-text-primary)]">
        🖼 AI 生图
      </h1>

      <div className="flex gap-6 flex-1 min-h-0">
        {/* Left panel: prompt + settings */}
        <div className="w-80 shrink-0 flex flex-col gap-4">
          <div>
            <label className="mb-2 block text-xs font-medium text-[var(--color-text-secondary)]">
              提示词 (Prompt)
            </label>
            <textarea
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              placeholder="描述你想生成的图片..."
              rows={5}
              className="w-full resize-none rounded border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] px-3 py-2 text-sm text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)] focus:border-[var(--color-accent)]"
            />
          </div>

          <div className="flex gap-3">
            <div className="flex-1">
              <label className="mb-1 block text-xs text-[var(--color-text-muted)]">宽高比</label>
              <select
                value={aspectRatio}
                onChange={(e) => setAspectRatio(e.target.value)}
                className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none"
              >
                {ASPECT_RATIOS.map((r) => (
                  <option key={r} value={r}>{r}</option>
                ))}
              </select>
            </div>
            <div className="flex-1">
              <label className="mb-1 block text-xs text-[var(--color-text-muted)]">分辨率</label>
              <select
                value={resolution}
                onChange={(e) => setResolution(e.target.value)}
                className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none"
              >
                {RESOLUTIONS.map((r) => (
                  <option key={r} value={r}>{r}</option>
                ))}
              </select>
            </div>
            <div className="w-16">
              <label className="mb-1 block text-xs text-[var(--color-text-muted)]">数量</label>
              <input
                type="number"
                min={1}
                max={4}
                value={n}
                onChange={(e) => setN(Math.max(1, Math.min(4, Number(e.target.value))))}
                className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none"
              />
            </div>
          </div>

          <button
            onClick={handleGenerate}
            disabled={generating || !prompt.trim()}
            className="rounded bg-[var(--color-accent)] px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-[var(--color-accent-hover)] disabled:opacity-50 active:scale-[0.97]"
          >
            {generating ? "生成中..." : "生成图片"}
          </button>

          {error && (
            <div className="rounded border border-[var(--color-danger)]/20 bg-[var(--color-danger-soft)] px-3 py-2 text-xs text-[var(--color-danger)]">
              {error}
            </div>
          )}
        </div>

        {/* Right panel: results */}
        <div className="flex-1 min-w-0 flex flex-col">
          {staged.length > 0 && (
            <>
              <div className="flex items-center justify-between mb-3">
                <span className="text-xs text-[var(--color-text-secondary)]">
                  已生成 {staged.length} 张 · 已选 {selected.length}/{staged.length}
                </span>
                <div className="flex gap-2">
                  <button onClick={selectAll} className="text-[11px] text-[var(--color-accent)] hover:underline">
                    全选
                  </button>
                  <button onClick={deselectAll} className="text-[11px] text-[var(--color-text-muted)] hover:underline">
                    取消全选
                  </button>
                </div>
              </div>
              <div className="flex-1 grid grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-3 overflow-auto content-start">
                {staged.map((img) => (
                  <div
                    key={img.id}
                    onClick={() => toggleSelect(img.id)}
                    className={`relative cursor-pointer rounded-xl border overflow-hidden transition-all duration-150 ${
                      selectedIds.has(img.id)
                        ? "border-[var(--color-accent)] ring-1 ring-[var(--color-accent)]"
                        : "border-[var(--color-border)] hover:border-[var(--color-accent)]/50"
                    }`}
                  >
                    <div
                      className="aspect-square bg-[var(--color-bg-tertiary)] flex items-center justify-center overflow-hidden"
                      onDoubleClick={(e) => { e.stopPropagation(); setPreview(img.path); }}
                    >
                      <img
                        src={convertFileSrc(img.path)}
                        alt=""
                        className="w-full h-full object-cover"
                        draggable={false}
                        decoding="async"
                      />
                    </div>
                    {/* Checkbox */}
                    <div className="absolute top-2 right-2 z-10">
                      <div className={`flex h-5 w-5 items-center justify-center rounded border-2 transition-all ${
                        selectedIds.has(img.id)
                          ? "border-[var(--color-accent)] bg-[var(--color-accent)] text-white"
                          : "border-white/50 bg-black/20 text-transparent"
                      }`}>
                        <svg className="h-3 w-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={3}>
                          <path strokeLinecap="round" strokeLinejoin="round" d="m4.5 12.75 6 6 9-13.5" />
                        </svg>
                      </div>
                    </div>
                  </div>
                ))}
              </div>
              <div className="flex items-center gap-3 pt-3 border-t border-[var(--color-border)] mt-3">
                <button
                  onClick={handleDiscard}
                  disabled={staged.length === 0}
                  className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-3 py-1.5 text-xs text-[var(--color-text-secondary)] transition-colors hover:bg-[var(--color-bg-hover)] disabled:opacity-50 active:scale-[0.97]"
                >
                  放弃未选中
                </button>
                <button
                  onClick={handleImport}
                  disabled={selected.length === 0 || importing}
                  className="rounded bg-[var(--color-accent)] px-4 py-1.5 text-xs font-medium text-white transition-colors hover:bg-[var(--color-accent-hover)] disabled:opacity-50 active:scale-[0.97]"
                >
                  {importing ? "导入中..." : `导入选中的 ${selected.length} 张 →`}
                </button>
              </div>
            </>
          )}
          {staged.length === 0 && !generating && (
            <div className="flex-1 flex items-center justify-center text-sm text-[var(--color-text-muted)]">
              输入提示词并点击 "生成图片" 开始
            </div>
          )}
          {generating && (
            <div className="flex-1 flex items-center justify-center text-sm text-[var(--color-text-muted)]">
              <span className="animate-pulse">正在生成中...</span>
            </div>
          )}
        </div>
      </div>
    </div>

    {/* Fullscreen preview overlay */}
    {preview && (
      <div
        ref={previewRef}
        className="fixed inset-0 z-50 bg-black/85 flex items-center justify-center animate-fade-in"
        onClick={() => setPreview(null)}
      >
        <img
          src={convertFileSrc(preview)}
          alt=""
          className="max-h-[90vh] max-w-[90vw] object-contain select-none"
          draggable={false}
        />
        <button
          onClick={() => setPreview(null)}
          className="absolute top-4 right-4 rounded-full bg-white/10 p-2 text-white/80 hover:bg-white/20 transition-colors"
        >
          <svg className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}><path strokeLinecap="round" strokeLinejoin="round" d="M6 18 18 6M6 6l12 12" /></svg>
        </button>
      </div>
    )}
    </>
  );
}

export default AiGenPage;
