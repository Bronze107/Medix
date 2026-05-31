import { useEffect, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { useThumbnail } from "@/hooks/useThumbnail";
import { imageEdit, imageConfirmImport, imageDiscardStaged } from "@/lib/tauri";
import type { StagedImage } from "@/lib/tauri";

interface Props {
  mediaId: string;
  onClose: () => void;
}

function ImagineDialog({ mediaId, onClose }: Props) {
  const [prompt, setPrompt] = useState("");
  const [resolution, setResolution] = useState("1k");
  const [n, setN] = useState(1);
  const [generating, setGenerating] = useState(false);
  const [staged, setStaged] = useState<StagedImage[]>([]);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [importing, setImporting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [imported, setImported] = useState(false);
  const [preview, setPreview] = useState<string | null>(null);

  const thumbUrl = useThumbnail(mediaId);

  useEffect(() => {
    if (staged.length > 0) setSelectedIds(new Set(staged.map((s) => s.id)));
  }, [staged]);

  const toggleSelect = (id: string) => {
    setSelectedIds((prev) => { const next = new Set(prev); if (next.has(id)) next.delete(id); else next.add(id); return next; });
  };

  const handleGenerate = async () => {
    if (!prompt.trim()) return;
    setGenerating(true);
    setError(null);
    try {
      const results = await imageEdit(mediaId, prompt.trim(), resolution, n);
      setStaged(results);
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
      await imageConfirmImport(toImport, prompt.trim(), mediaId);
      setImported(true);
    } catch (e) {
      setError(String(e));
    } finally {
      setImporting(false);
    }
  };

  const handleDiscard = async () => {
    const unselected = staged.filter((s) => !selectedIds.has(s.id)).map((s) => s.id);
    const toDiscard = unselected.length > 0 ? unselected : staged.map((s) => s.id);
    await imageDiscardStaged(toDiscard);
    if (unselected.length === 0) {
      setStaged([]);
      setSelectedIds(new Set());
    } else {
      setStaged((prev) => prev.filter((s) => selectedIds.has(s.id)));
    }
  };

  const selected = Array.from(selectedIds);

  return (
    <>
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-[var(--color-bg-overlay)] animate-fade-in" onClick={onClose}>
      <div className="w-[640px] max-h-[85vh] rounded-xl bg-[var(--color-bg-elevated)] border border-[var(--color-border)] shadow-2xl animate-scale-in flex flex-col" onClick={(e) => e.stopPropagation()}>
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-4 border-b border-[var(--color-border)]">
          <h2 className="text-sm font-semibold text-[var(--color-text-primary)]">AI 图像编辑</h2>
          <button onClick={onClose} className="rounded p-1 text-[var(--color-text-muted)] hover:bg-[var(--color-bg-hover)] hover:text-[var(--color-text-primary)] transition-colors">
            <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}><path strokeLinecap="round" strokeLinejoin="round" d="M6 18 18 6M6 6l12 12" /></svg>
          </button>
        </div>

        <div className="flex-1 overflow-auto p-5">
          {imported ? (
            <div className="flex flex-col items-center justify-center py-8">
              <svg className="mb-3 h-10 w-10 text-[var(--color-success)]" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}><path strokeLinecap="round" strokeLinejoin="round" d="m4.5 12.75 6 6 9-13.5" /></svg>
              <p className="text-sm text-[var(--color-text-secondary)]">已导入，新版本可在版本标签页查看</p>
              <button onClick={onClose} className="mt-4 rounded bg-[var(--color-accent)] px-4 py-1.5 text-xs font-medium text-white hover:bg-[var(--color-accent-hover)]">关闭</button>
            </div>
          ) : (
            <div className="flex gap-4">
              {/* Thumbnail */}
              <div className="w-28 h-28 shrink-0 rounded-lg overflow-hidden bg-[var(--color-bg-tertiary)]">
                {thumbUrl ? <img src={thumbUrl} alt="" className="w-full h-full object-cover" decoding="async" /> : (
                  <div className="w-full h-full flex items-center justify-center text-[var(--color-text-muted)] text-[11px]">原图</div>
                )}
              </div>

              {/* Input area */}
              <div className="flex-1 min-w-0 space-y-3">
                <div>
                  <label className="mb-1 block text-xs text-[var(--color-text-secondary)]">编辑指令</label>
                  <textarea
                    value={prompt}
                    onChange={(e) => setPrompt(e.target.value)}
                    placeholder='例如："转为黑白素描风格"'
                    rows={3}
                    className="w-full resize-none rounded border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] px-3 py-2 text-sm text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)] focus:border-[var(--color-accent)]"
                  />
                </div>
                <div className="flex items-center gap-3">
                  <div className="w-24">
                    <select value={resolution} onChange={(e) => setResolution(e.target.value)}
                      className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none">
                      <option value="1k">1K</option>
                      <option value="2k">2K</option>
                    </select>
                  </div>
                  <div className="w-16">
                    <input type="number" min={1} max={4} value={n}
                      onChange={(e) => setN(Math.max(1, Math.min(4, Number(e.target.value))))}
                      className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none" />
                  </div>
                  <button onClick={handleGenerate} disabled={generating || !prompt.trim()}
                    className="rounded bg-[var(--color-accent)] px-3 py-1.5 text-xs font-medium text-white transition-colors hover:bg-[var(--color-accent-hover)] disabled:opacity-50 active:scale-[0.97]">
                    {generating ? "生成中..." : "生成"}
                  </button>
                </div>
                {error && <div className="rounded border border-[var(--color-danger)]/20 bg-[var(--color-danger-soft)] px-3 py-1.5 text-xs text-[var(--color-danger)]">{error}</div>}
              </div>
            </div>
          )}

          {/* Results */}
          {!imported && staged.length > 0 && (
            <div className="mt-4">
              <div className="flex items-center justify-between mb-2">
                <span className="text-xs text-[var(--color-text-secondary)]">已生成 {staged.length} 张 · 已选 {selected.length}</span>
              </div>
              <div className="grid grid-cols-2 gap-2 max-h-64 overflow-auto">
                {staged.map((img) => (
                  <div key={img.id} onClick={() => toggleSelect(img.id)}
                    className={`cursor-pointer rounded-lg border overflow-hidden transition-all ${
                      selectedIds.has(img.id) ? "border-[var(--color-accent)] ring-1 ring-[var(--color-accent)]" : "border-[var(--color-border)] hover:border-[var(--color-accent)]/50"
                    }`}>
                    <div
                      className="aspect-square bg-[var(--color-bg-tertiary)]"
                      onDoubleClick={(e) => { e.stopPropagation(); setPreview(img.path); }}
                    >
                      <img src={convertFileSrc(img.path)} alt="" className="w-full h-full object-cover" draggable={false} decoding="async" />
                    </div>
                    <div className="px-2 py-1 text-[10px] text-[var(--color-text-muted)] text-center">
                      {img.width} × {img.height}
                    </div>
                  </div>
                ))}
              </div>
              <div className="flex items-center gap-2 mt-3">
                <button onClick={handleDiscard} disabled={staged.length === 0}
                  className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)] disabled:opacity-50">放弃未选中</button>
                <button onClick={handleImport} disabled={selected.length === 0 || importing}
                  className="rounded bg-[var(--color-accent)] px-3 py-1 text-xs font-medium text-white hover:bg-[var(--color-accent-hover)] disabled:opacity-50">
                  {importing ? "导入中..." : `导入 ${selected.length} 张 →`}
                </button>
              </div>
            </div>
          )}

          {!imported && staged.length === 0 && generating && (
            <div className="py-8 text-center text-sm text-[var(--color-text-muted)] animate-pulse">正在生成中...</div>
          )}
        </div>
      </div>
    </div>

    {/* Fullscreen preview overlay */}
    {preview && (
      <div
        className="fixed inset-0 z-[60] bg-black/85 flex items-center justify-center animate-fade-in"
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

export default ImagineDialog;
