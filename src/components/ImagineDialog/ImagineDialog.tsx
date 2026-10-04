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
import {
  clearWorkflowValues,
  defaultWorkflowValues,
  mergeWorkflowValues,
  rememberWorkflowValues,
} from "@/lib/workflowValueMemory";
import { showToast } from "@/components/Toast/Toast";
import type { ComfyWorkflow, WorkflowParam } from "@/types/comfyui";
import { ComfyUIWorkflowForm, ComfyUIWorkflowParams } from "@/components/shared/ComfyUIForm";

interface Props {
  mediaId: string;
  sourceMediaIds?: string[];
  sourceMediaPath?: string;
  onClose: () => void;
}

function ImagineDialog({ mediaId, sourceMediaIds, sourceMediaPath, onClose }: Props) {
  const [prompt, setPrompt] = useState("");
  const [aspectRatio, setAspectRatio] = useState("auto");
  const [resolution, setResolution] = useState("1k");
  const [n, setN] = useState(1);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const { items: history, record, clear } = usePromptHistory("edit");

  // ComfyUI state
  const [provider, setProvider] = useState<string>("");
  const [providerLoading, setProviderLoading] = useState(true);
  const [workflows, setWorkflows] = useState<ComfyWorkflow[]>([]);
  const [selectedWorkflowId, setSelectedWorkflowId] = useState("");
  const [workflowParams, setWorkflowParams] = useState<WorkflowParam[]>([]);
  const [workflowParamsLoading, setWorkflowParamsLoading] = useState(false);
  const [workflowParamsError, setWorkflowParamsError] = useState<string | null>(null);
  const [workflowValues, setWorkflowValues] = useState<Record<string, string>>({});

  const isComfy = provider === "comfyui";
  const comfyReady = !isComfy || (isComfy && selectedWorkflowId && workflows.length > 0 && !providerLoading);
  const comfyPrompt = (() => {
    const p = workflowParams.find(
      (x) => x.field_type === "multiline" || x.field_type === "text",
    );
    return p ? workflowValues[p.param_name] ?? p.default_value ?? "" : "";
  })();

  const editMediaIds = sourceMediaIds ?? [mediaId];

  const thumbUrl = sourceMediaPath ? convertFileSrc(sourceMediaPath) : useThumbnail(mediaId);

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
      setProviderLoading(false);
    }).catch(() => {
      setProviderLoading(false);
    });
  }, []);

  // Load workflow params when selection changes
  useEffect(() => {
    setWorkflowParams([]);
    setWorkflowValues({});
    setWorkflowParamsError(null);
    if (!selectedWorkflowId) return;
    setWorkflowParamsLoading(true);
    comfyuiWorkflowGet(selectedWorkflowId).then((detail) => {
      setWorkflowParams(detail.params);
      setWorkflowValues(mergeWorkflowValues(detail.id, detail.params));
      setWorkflowParamsError(null);
    }).catch((e) => {
      setWorkflowParamsError("加载工作流参数失败: " + (e?.message || e));
      setWorkflowParams([]);
      setWorkflowValues({});
    }).finally(() => {
      setWorkflowParamsLoading(false);
    });
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
        editMediaIds,
        isComfy ? comfyPrompt : prompt.trim(),
        isComfy ? workflowValues : undefined,
        aspectRatio,
        resolution,
        n,
        isComfy ? selectedWorkflowId : null,
      );
      // 提交成功后才记住本次参数（「上一次」= 上一次真正跑过的值）
      if (isComfy && selectedWorkflowId) {
        rememberWorkflowValues(selectedWorkflowId, workflowParams, workflowValues);
      }
      record(isComfy ? comfyPrompt : prompt, aspectRatio, resolution);
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
              <div className="relative w-40 h-40 shrink-0 rounded-lg overflow-hidden bg-[var(--color-bg-tertiary)]">
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
                {editMediaIds.length > 1 && (
                  <div className="absolute bottom-1 left-1 right-1 rounded bg-black/60 px-1.5 py-0.5 text-[10px] text-white text-center">
                    已选择 {editMediaIds.length} 张输入图
                  </div>
                )}
              </div>

              {/* Input area */}
              <div className="flex-1 min-w-0 space-y-3">
                {/* ComfyUI mode: workflow selector + dynamic params */}
                {isComfy ? (
                  <>
                    <ComfyUIWorkflowForm
                      workflows={workflows}
                      selectedWorkflowId={selectedWorkflowId}
                      onWorkflowChange={setSelectedWorkflowId}
                      loading={workflowParamsLoading}
                      error={workflowParamsError}
                      emptyMessage={'暂无图生图工作流。请在 ComfyUI 中搭建 <strong>App 模式</strong>工作流（含 extra.linearData），再到 <strong>设置 → ComfyUI 配置</strong> 中粘贴保存。'}
                    />
                    {workflowParams.length > 0 && !workflowParamsLoading && !workflowParamsError && (
                      <ComfyUIWorkflowParams
                        params={workflowParams}
                        values={workflowValues}
                        setValues={setWorkflowValues}
                        mode="edit"
                        onReset={() => {
                          clearWorkflowValues(selectedWorkflowId);
                          setWorkflowValues(defaultWorkflowValues(workflowParams));
                        }}
                      />
                    )}
                  </>
                ) : (
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

export default ImagineDialog;
