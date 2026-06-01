import { useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { useThumbnail } from "@/hooks/useThumbnail";
import { imageQueueSubmitEdit } from "@/lib/tauri";
import { usePromptHistory } from "@/hooks/usePromptHistory";

interface Props {
  mediaId: string;
  variantId?: string | null;
  variantPath?: string | null;
  onClose: () => void;
}

function ImagineDialog({ mediaId, variantId, variantPath, onClose }: Props) {
  const [prompt, setPrompt] = useState("");
  const [aspectRatio, setAspectRatio] = useState("auto");
  const [resolution, setResolution] = useState("1k");
  const [n, setN] = useState(1);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [submitted, setSubmitted] = useState(false);
  const { items: history, record, clear } = usePromptHistory("edit");

  const thumbUrl = variantPath ? convertFileSrc(variantPath) : useThumbnail(mediaId);

  const handleSubmit = async () => {
    if (!prompt.trim()) return;
    setSubmitting(true);
    setError(null);
    try {
      await imageQueueSubmitEdit(
        mediaId,
        variantId ?? null,
        prompt.trim(),
        aspectRatio,
        resolution,
        n,
      );
      record(prompt, aspectRatio, resolution);
      setSubmitted(true);
    } catch (e) {
      setError(String(e));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <>
      <div
        className="fixed inset-0 z-50 flex items-center justify-center bg-[var(--color-bg-overlay)] animate-fade-in"
        onClick={submitted ? onClose : onClose}
      >
        <div
          className="w-[560px] max-h-[85vh] rounded-xl bg-[var(--color-bg-elevated)] border border-[var(--color-border)] shadow-2xl animate-scale-in flex flex-col"
          onClick={(e) => e.stopPropagation()}
        >
          {/* Header */}
          <div className="flex items-center justify-between px-5 py-4 border-b border-[var(--color-border)]">
            <h2 className="text-sm font-semibold text-[var(--color-text-primary)]">
              AI 图像编辑
            </h2>
            <button
              onClick={onClose}
              className="rounded p-1 text-[var(--color-text-muted)] hover:bg-[var(--color-bg-hover)] hover:text-[var(--color-text-primary)] transition-colors"
            >
              <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M6 18 18 6M6 6l12 12" />
              </svg>
            </button>
          </div>

          <div className="flex-1 overflow-auto p-5">
            {submitted ? (
              <div className="flex flex-col items-center justify-center py-6">
                <svg
                  className="mb-3 h-10 w-10 text-[var(--color-success)]"
                  fill="none"
                  viewBox="0 0 24 24"
                  stroke="currentColor"
                  strokeWidth={2}
                >
                  <path strokeLinecap="round" strokeLinejoin="round" d="m4.5 12.75 6 6 9-13.5" />
                </svg>
                <p className="text-sm text-[var(--color-text-secondary)]">
                  已加入队列，后台生成中
                </p>
                <p className="mt-1 text-xs text-[var(--color-text-muted)]">
                  完成后可在 AI 生图页面查看结果
                </p>
                <button
                  onClick={onClose}
                  className="mt-4 rounded bg-[var(--color-accent)] px-4 py-1.5 text-xs font-medium text-white hover:bg-[var(--color-accent-hover)] transition-colors"
                >
                  关闭
                </button>
              </div>
            ) : (
              <div className="flex gap-4">
                {/* Thumbnail */}
                <div className="w-40 h-40 shrink-0 rounded-lg overflow-hidden bg-[var(--color-bg-tertiary)]">
                  {thumbUrl ? (
                    <img
                      src={thumbUrl}
                      alt=""
                      className="w-full h-full object-cover"
                      decoding="async"
                    />
                  ) : (
                    <div className="w-full h-full flex items-center justify-center text-[var(--color-text-muted)] text-[11px]">
                      原图
                    </div>
                  )}
                </div>

                {/* Input area */}
                <div className="flex-1 min-w-0 space-y-3">
                  <div>
                    <label className="mb-1 block text-xs text-[var(--color-text-secondary)]">
                      编辑指令
                    </label>
                    <textarea
                      value={prompt}
                      onChange={(e) => setPrompt(e.target.value)}
                      placeholder='例如："转为黑白素描风格"（Ctrl+Enter 提交）'
                      rows={3}
                      className="w-full resize-none rounded border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] px-3 py-2 text-sm text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)] focus:border-[var(--color-accent)]"
                    />
                    {history.length > 0 && (
                      <div className="mt-1.5 flex items-start gap-1.5 flex-wrap">
                        <span className="text-[10px] text-[var(--color-text-muted)] shrink-0 leading-5">历史</span>
                        {history.slice(0, 4).map((h) => (
                          <button key={h.time}
                            onClick={() => { setPrompt(h.prompt); setResolution(h.resolution); setAspectRatio(h.aspectRatio); }}
                            className="max-w-[140px] truncate rounded-full border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-0.5 text-[10px] text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)] hover:text-[var(--color-text-primary)] transition-colors"
                            title={h.prompt}
                          >{h.prompt}</button>
                        ))}
                        <button onClick={clear}
                          className="shrink-0 text-[10px] text-[var(--color-text-muted)] hover:text-[var(--color-danger)] transition-colors leading-5"
                        >清除</button>
                      </div>
                    )}
                  </div>
                  <div className="flex items-center gap-2">
                    <div className="w-20">
                      <select
                        value={aspectRatio}
                        onChange={(e) => setAspectRatio(e.target.value)}
                        className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-1.5 py-1.5 text-xs text-[var(--color-text-primary)] outline-none"
                      >
                        <option value="auto">auto</option>
                        <option value="1:1">1:1</option>
                        <option value="4:3">4:3</option>
                        <option value="3:4">3:4</option>
                        <option value="16:9">16:9</option>
                        <option value="9:16">9:16</option>
                        <option value="3:2">3:2</option>
                        <option value="2:3">2:3</option>
                      </select>
                    </div>
                    <div className="w-16">
                      <select
                        value={resolution}
                        onChange={(e) => setResolution(e.target.value)}
                        className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-1.5 py-1.5 text-xs text-[var(--color-text-primary)] outline-none"
                      >
                        <option value="1k">1K</option>
                        <option value="2k">2K</option>
                      </select>
                    </div>
                    <div className="w-16">
                      <input
                        type="number"
                        min={1}
                        max={4}
                        value={n}
                        onChange={(e) =>
                          setN(Math.max(1, Math.min(4, Number(e.target.value))))
                        }
                        className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none"
                      />
                    </div>
                    <button
                      onClick={handleSubmit}
                      disabled={submitting || !prompt.trim()}
                      className="rounded bg-[var(--color-accent)] px-3 py-1.5 text-xs font-medium text-white transition-colors hover:bg-[var(--color-accent-hover)] disabled:opacity-50 active:scale-[0.97]"
                    >
                      {submitting
                        ? "提交中..."
                        : prompt.trim()
                          ? "加入队列"
                          : "输入指令后生成"}
                    </button>
                  </div>
                  {error && (
                    <div className="rounded border border-[var(--color-danger)]/20 bg-[var(--color-danger-soft)] px-3 py-1.5 text-xs text-[var(--color-danger)]">
                      {error}
                    </div>
                  )}
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </>
  );
}

export default ImagineDialog;
