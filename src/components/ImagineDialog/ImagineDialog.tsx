import { useEffect, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { useThumbnail } from "@/hooks/useThumbnail";
import {
  imageQueueSubmitEdit,
  comfyuiWorkflowList,
  comfyuiWorkflowGet,
  settingsGet,
} from "@/lib/tauri";
import { usePromptHistory } from "@/hooks/usePromptHistory";
import { showToast } from "@/components/Toast/Toast";
import type { ComfyWorkflow, WorkflowParam } from "@/types/comfyui";

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
  const { items: history, record, clear } = usePromptHistory("edit");

  // ComfyUI state
  const [provider, setProvider] = useState<string>("");
  const [workflows, setWorkflows] = useState<ComfyWorkflow[]>([]);
  const [selectedWorkflowId, setSelectedWorkflowId] = useState("");
  const [workflowParams, setWorkflowParams] = useState<WorkflowParam[]>([]);
  const [workflowValues, setWorkflowValues] = useState<Record<string, string>>({});

  const isComfy = provider === "comfyui";
  const comfyReady = !isComfy || (isComfy && selectedWorkflowId && workflows.length > 0);

  const thumbUrl = variantPath ? convertFileSrc(variantPath) : useThumbnail(mediaId);

  // Detect provider + load edit workflows
  useEffect(() => {
    settingsGet("image_api_provider").then((v) => {
      const p = v || "";
      setProvider(p);
      if (p === "comfyui") {
        comfyuiWorkflowList("edit").then((list) => {
          setWorkflows(list);
          if (list.length > 0) {
            setSelectedWorkflowId(list[0].id);
          }
        }).catch(() => {});
      }
    }).catch(() => {});
  }, []);

  // Load workflow params when selection changes
  useEffect(() => {
    if (!selectedWorkflowId) return;
    comfyuiWorkflowGet(selectedWorkflowId).then((detail) => {
      setWorkflowParams(detail.params);
      const init: Record<string, string> = {};
      for (const p of detail.params) {
        init[p.param_name] = p.default_value;
      }
      setWorkflowValues(init);
    }).catch(console.error);
  }, [selectedWorkflowId]);

  const handleSubmit = async () => {
    if (isComfy) {
      if (!selectedWorkflowId) return;
    } else {
      if (!prompt.trim()) return;
    }
    setSubmitting(true);
    setError(null);
    try {
      await imageQueueSubmitEdit(
        mediaId,
        variantId ?? null,
        isComfy ? (workflowValues.prompt || prompt.trim()) : prompt.trim(),
        aspectRatio,
        resolution,
        n,
        isComfy ? selectedWorkflowId : null,
      );
      record(prompt, aspectRatio, resolution);
      showToast("已加入队列");
      onClose();
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
        onClick={onClose}
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
                {/* ComfyUI mode: workflow selector + dynamic params */}
                {isComfy && workflows.length > 0 && (
                  <div>
                    <label className="mb-1 block text-xs text-[var(--color-text-secondary)]">
                      工作流
                    </label>
                    <select
                      value={selectedWorkflowId}
                      onChange={(e) => setSelectedWorkflowId(e.target.value)}
                      className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none"
                    >
                      {workflows.map((w) => (
                        <option key={w.id} value={w.id}>{w.name}</option>
                      ))}
                    </select>
                  </div>
                )}

                {isComfy && workflows.length === 0 && (
                  <p className="text-xs text-[var(--color-text-muted)]">
                    暂无图生图工作流。请先到 <strong>设置 → ComfyUI 配置</strong> 中保存 workflow。
                  </p>
                )}

                {isComfy
                  ? workflowParams.map((p) => (
                      <div key={p.node_id + p.param_name}>
                        <label className="mb-1 block text-xs text-[var(--color-text-secondary)]">
                          #{p.param_name}
                        </label>
                        {renderDynamicField(p, workflowValues, setWorkflowValues)}
                      </div>
                    ))
                  : (
                    /* Non-ComfyUI mode: prompt + settings */
                    <>
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
                    </>
                  )}

                {/* Shared controls (non-ComfyUI only) */}
                {!isComfy && (
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
                  </div>
                )}

                <div className="flex items-center gap-2">
                  <button
                    onClick={handleSubmit}
                    disabled={submitting || !comfyReady || (!isComfy && !prompt.trim())}
                    className="rounded bg-[var(--color-accent)] px-3 py-1.5 text-xs font-medium text-white transition-colors hover:bg-[var(--color-accent-hover)] disabled:opacity-50 active:scale-[0.97]"
                  >
                    {submitting
                      ? "提交中..."
                      : !comfyReady
                        ? "请先在设置中保存工作流"
                        : prompt.trim() || isComfy
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
          </div>
        </div>
      </div>
    </>
  );
}

function renderDynamicField(
  p: WorkflowParam,
  values: Record<string, string>,
  setValues: (fn: (prev: Record<string, string>) => Record<string, string>) => void,
) {
  switch (p.field_type) {
    case "multiline":
      return (
        <textarea
          value={values[p.param_name] ?? ""}
          onChange={(e) => setValues(v => ({ ...v, [p.param_name]: e.target.value }))}
          rows={3}
          className="w-full resize-none rounded border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] px-3 py-2 text-sm text-[var(--color-text-primary)] outline-none focus:border-[var(--color-accent)]"
        />
      );
    case "seed":
      return (
        <div className="flex gap-2">
          <input
            type="number"
            value={values[p.param_name] ?? ""}
            onChange={(e) => setValues(v => ({ ...v, [p.param_name]: e.target.value }))}
            className="flex-1 rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none"
          />
          <button
            onClick={() => setValues(v => ({ ...v, [p.param_name]: "-1" }))}
            className="shrink-0 rounded border border-[var(--color-border-light)] px-2 py-1 text-[11px] text-[var(--color-text-muted)] hover:bg-[var(--color-bg-hover)] active:scale-[0.97]"
          >
            🎲
          </button>
        </div>
      );
    case "slider":
      return (
        <div className="flex items-center gap-2">
          <input
            type="range"
            min={1}
            max={p.param_name === "steps" ? 100 : p.param_name === "cfg" ? 30 : 100}
            step={p.param_name === "cfg" ? 0.5 : 1}
            value={parseFloat(values[p.param_name] || "1")}
            onChange={(e) => setValues(v => ({ ...v, [p.param_name]: e.target.value }))}
            className="flex-1"
          />
          <span className="w-10 text-right text-xs text-[var(--color-text-secondary)]">{values[p.param_name]}</span>
        </div>
      );
    case "image_selector":
      return (
        <div className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-xs text-[var(--color-text-muted)]">
          当前选中的图片（自动绑定原图）
        </div>
      );
    default:
      return (
        <input
          type={p.field_type === "number" ? "number" : "text"}
          value={values[p.param_name] ?? ""}
          onChange={(e) => setValues(v => ({ ...v, [p.param_name]: e.target.value }))}
          className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none"
        />
      );
  }
}

export default ImagineDialog;
