import { useCallback, useEffect, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  imageQueueSubmitGenerate,
  imageQueueList,
  imageQueueImport,
  imageQueueDiscard,
  imageQueueDismiss,
} from "@/lib/tauri";
import { usePromptHistory } from "@/hooks/usePromptHistory";
import type { ImageTaskInfo } from "@/lib/tauri";

const ASPECT_RATIOS = ["auto", "1:1", "4:3", "3:4", "16:9", "9:16", "2:3", "3:2", "1:2", "2:1"];
const RESOLUTIONS = ["1k", "2k"];

function TaskCard({
  task,
  onImport,
  onDiscard,
  onDismiss,
  onPreview,
}: {
  task: ImageTaskInfo;
  onImport: (taskId: string, selectedIds: string[]) => void;
  onDiscard: (taskId: string) => void;
  onDismiss: (taskId: string) => void;
  onPreview: (path: string) => void;
}) {
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [importing, setImporting] = useState(false);

  useEffect(() => {
    if (task.staged.length > 0) {
      setSelected(new Set(task.staged.map((s) => s.id)));
    }
  }, [task.staged]);

  const toggle = (id: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const statusConfig: Record<string, { label: string; color: string }> = {
    pending: { label: "排队中", color: "var(--color-text-muted)" },
    running: { label: "生成中", color: "var(--color-accent)" },
    done: { label: "已完成", color: "var(--color-success)" },
    failed: { label: "失败", color: "var(--color-danger)" },
  };
  const sc = statusConfig[task.status] || statusConfig.pending;
  const isRunning = task.status === "pending" || task.status === "running";

  return (
    <div className="rounded-xl border border-[var(--color-border)] bg-[var(--color-bg-elevated)] p-3">
      {/* Header */}
      <div className="flex items-center justify-between gap-2 mb-2">
        <span className="min-w-0 truncate text-xs text-[var(--color-text-primary)]">
          {task.prompt}
        </span>
        <span className="shrink-0 text-[11px]" style={{ color: sc.color }}>
          {isRunning && (
            <svg className="inline h-3 w-3 animate-spin mr-1" viewBox="0 0 24 24" fill="none">
              <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
              <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
            </svg>
          )}
          {sc.label}
        </span>
      </div>

      {/* Done: results grid */}
      {task.status === "done" && task.staged.length > 0 && (
        <>
          <div className="grid grid-cols-4 gap-2 mb-2">
            {task.staged.map((img) => (
              <div
                key={img.id}
                onClick={() => toggle(img.id)}
                className={`cursor-pointer rounded-lg border overflow-hidden transition-all ${
                  selected.has(img.id)
                    ? "border-[var(--color-accent)] ring-1 ring-[var(--color-accent)]"
                    : "border-[var(--color-border)] hover:border-[var(--color-accent)]/50"
                }`}
              >
                <div
                  className="aspect-square bg-[var(--color-bg-tertiary)]"
                  onDoubleClick={(e) => {
                    e.stopPropagation();
                    onPreview(img.path);
                  }}
                >
                  <img
                    src={convertFileSrc(img.path)}
                    alt=""
                    className="w-full h-full object-cover"
                    draggable={false}
                    decoding="async"
                  />
                </div>
                <div className="px-1 py-0.5 text-[11px] text-[var(--color-text-muted)] text-center">
                  {img.width}×{img.height}
                </div>
              </div>
            ))}
          </div>
          <div className="flex items-center gap-2">
            <button
              onClick={async () => {
                const ids = Array.from(selected);
                if (ids.length === 0) return;
                setImporting(true);
                try {
                  await onImport(task.task_id, ids);
                } finally {
                  setImporting(false);
                }
              }}
              disabled={selected.size === 0 || importing}
              className="rounded bg-[var(--color-accent)] px-2 py-1 text-[11px] font-medium text-white hover:bg-[var(--color-accent-hover)] disabled:opacity-50 active:scale-[0.97]"
            >
              {importing ? "导入中..." : `导入 ${selected.size} 张`}
            </button>
            <button
              onClick={() => onDiscard(task.task_id)}
              className="rounded border border-[var(--color-border-light)] px-2 py-1 text-[11px] text-[var(--color-text-muted)] hover:bg-[var(--color-bg-hover)] transition-colors active:scale-[0.97]"
            >
              丢弃
            </button>
          </div>
        </>
      )}

      {/* Failed: error + dismiss */}
      {task.status === "failed" && task.error && (
        <>
          <p className="mb-2 text-[11px] text-[var(--color-danger)] line-clamp-3">{task.error}</p>
          <button
            onClick={() => onDismiss(task.task_id)}
            className="rounded border border-[var(--color-border-light)] px-2 py-1 text-[11px] text-[var(--color-text-muted)] hover:bg-[var(--color-bg-hover)] transition-colors active:scale-[0.97]"
          >
            移除
          </button>
        </>
      )}
    </div>
  );
}

function AiGenPage() {
  const [prompt, setPrompt] = useState("");
  const [aspectRatio, setAspectRatio] = useState("auto");
  const [resolution, setResolution] = useState("1k");
  const [n, setN] = useState(1);
  const [tasks, setTasks] = useState<ImageTaskInfo[]>([]);
  const [submitting, setSubmitting] = useState(false);
  const [preview, setPreview] = useState<string | null>(null);
  const { items: history, record, clear } = usePromptHistory("generate");

  const loadTasks = useCallback(async () => {
    try {
      setTasks(await imageQueueList());
    } catch (e) {
      console.error("Failed to load image queue:", e);
    }
  }, []);

  useEffect(() => {
    loadTasks();
    const unlisten = listen("image-queue-updated", () => {
      loadTasks();
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [loadTasks]);

  const handleSubmit = async () => {
    if (!prompt.trim()) return;
    setSubmitting(true);
    try {
      await imageQueueSubmitGenerate(prompt.trim(), aspectRatio, resolution, n);
      record(prompt, aspectRatio, resolution);
      setPrompt("");
      await loadTasks();
    } catch (e) {
      console.error("Failed to submit:", e);
    } finally {
      setSubmitting(false);
    }
  };

  const handleImport = async (taskId: string, selectedIds: string[]) => {
    try {
      await imageQueueImport(taskId, selectedIds);
      await loadTasks();
    } catch (e) {
      console.error("Import failed:", e);
    }
  };

  const handleDiscard = async (taskId: string) => {
    await imageQueueDiscard(taskId);
    await loadTasks();
  };

  const handleDismiss = async (taskId: string) => {
    await imageQueueDismiss(taskId);
    await loadTasks();
  };

  const hasActive = tasks.some(
    (t) => t.status === "pending" || t.status === "running",
  );

  return (
    <>
      <div className="flex h-full flex-col p-6">
        <h1 className="mb-6 text-xl font-semibold text-[var(--color-text-primary)]">
          AI 生图
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
                onKeyDown={(e) => {
                  if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
                    e.preventDefault();
                    handleSubmit();
                  }
                }}
                placeholder="描述你想生成的图片...（Ctrl+Enter 提交）"
                rows={5}
                className="w-full resize-none rounded border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] px-3 py-2 text-sm text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)] focus:border-[var(--color-accent)]"
              />
              {history.length > 0 && (
                <div className="mt-1.5 flex items-start gap-1.5 flex-wrap">
                  <span className="text-[10px] text-[var(--color-text-muted)] shrink-0 leading-6">历史</span>
                  <div className="flex items-center gap-1 flex-wrap flex-1 min-w-0">
                    {history.slice(0, 6).map((h) => (
                      <button
                        key={h.time}
                        onClick={() => { setPrompt(h.prompt); setAspectRatio(h.aspectRatio); setResolution(h.resolution); }}
                        className="max-w-[170px] truncate rounded-full border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-0.5 text-[11px] text-[var(--color-text-secondary)] transition-colors hover:bg-[var(--color-bg-hover)] hover:text-[var(--color-text-primary)]"
                        title={h.prompt}
                      >
                        {h.prompt}
                      </button>
                    ))}
                  </div>
                  <button
                    onClick={clear}
                    className="shrink-0 text-[10px] text-[var(--color-text-muted)] hover:text-[var(--color-danger)] transition-colors leading-6"
                  >
                    清除
                  </button>
                </div>
              )}
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
              onClick={handleSubmit}
              disabled={submitting || !prompt.trim()}
              className="rounded bg-[var(--color-accent)] px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-[var(--color-accent-hover)] disabled:opacity-50 active:scale-[0.97]"
            >
              {submitting ? "提交中..." : prompt.trim() ? "加入队列" : "输入提示词后生成"}
            </button>

            {hasActive && (
              <p className="text-[11px] text-[var(--color-accent)]">
                有任务正在后台生成，可关闭此页面继续操作
              </p>
            )}
          </div>

          {/* Right panel: task list */}
          <div className="flex-1 min-w-0 overflow-auto">
            {tasks.length > 0 ? (
              <div className="space-y-3">
                {tasks.map((task) => (
                  <TaskCard
                    key={task.task_id}
                    task={task}
                    onImport={handleImport}
                    onDiscard={handleDiscard}
                    onDismiss={handleDismiss}
                    onPreview={setPreview}
                  />
                ))}
              </div>
            ) : (
              <div className="flex items-center justify-center h-full text-sm text-[var(--color-text-muted)]">
                输入提示词并点击"加入队列"开始
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Fullscreen preview overlay */}
      {preview && (
        <div
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
            <svg className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M6 18 18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
      )}
    </>
  );
}

export default AiGenPage;
