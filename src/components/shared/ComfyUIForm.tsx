import type { ComfyWorkflow, WorkflowParam } from "@/types/comfyui";

interface WorkflowFormProps {
  workflows: ComfyWorkflow[];
  selectedWorkflowId: string;
  onWorkflowChange: (id: string) => void;
  loading?: boolean;
  error?: string | null;
  emptyMessage: string;
}

export function ComfyUIWorkflowForm({
  workflows,
  selectedWorkflowId,
  onWorkflowChange,
  loading,
  error,
  emptyMessage,
}: WorkflowFormProps) {
  if (!workflows || workflows.length === 0) {
    return (
      <p className="text-xs text-[var(--color-text-muted)] py-3">{emptyMessage}</p>
    );
  }

  return (
    <>
      <div>
        <label className="mb-1 block text-xs text-[var(--color-text-muted)]">工作流</label>
        <select
          value={selectedWorkflowId}
          onChange={(e) => onWorkflowChange(e.target.value)}
          className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none"
        >
          {workflows.map((w) => (
            <option key={w.id} value={w.id}>{w.name}</option>
          ))}
        </select>
      </div>

      {loading && (
        <p className="text-xs text-[var(--color-text-muted)]">加载参数中...</p>
      )}
      {error && (
        <p className="text-xs text-[var(--color-danger)]">{error}</p>
      )}
    </>
  );
}

interface DynamicFieldProps {
  param: WorkflowParam;
  values: Record<string, string>;
  setValues: (fn: (prev: Record<string, string>) => Record<string, string>) => void;
}

export function ComfyUIDynamicField({ param, values, setValues }: DynamicFieldProps) {
  switch (param.field_type) {
    case "multiline":
      return (
        <textarea
          value={values[param.param_name] ?? param.default_value ?? ""}
          onChange={(e) => setValues((v) => ({ ...v, [param.param_name]: e.target.value }))}
          rows={4}
          className="w-full resize-none rounded border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] px-3 py-2 text-sm text-[var(--color-text-primary)] outline-none focus:border-[var(--color-accent)]"
        />
      );
    case "seed":
      return (
        <div className="flex gap-2">
          <input
            type="number"
            value={values[param.param_name] ?? param.default_value ?? ""}
            onChange={(e) => setValues((v) => ({ ...v, [param.param_name]: e.target.value }))}
            className="flex-1 rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none"
          />
          <button
            onClick={() => setValues((v) => ({ ...v, [param.param_name]: "-1" }))}
            className="shrink-0 rounded border border-[var(--color-border-light)] px-2 py-1 text-[11px] text-[var(--color-text-muted)] hover:bg-[var(--color-bg-hover)] active:scale-[0.97]"
          >
            🎲
          </button>
        </div>
      );
    case "slider": {
      const raw = values[param.param_name] ?? param.default_value ?? "1";
      const numVal = parseFloat(raw || "1");
      const safeVal = isNaN(numVal) ? 1 : numVal;
      const min = param.min ?? 1;
      const max = param.max ?? (param.param_name === "cfg" ? 30 : 100);
      const step = param.step ?? (param.param_name === "cfg" ? 0.5 : 1);
      return (
        <div className="flex items-center gap-2">
          <input
            type="range"
            min={min}
            max={max}
            step={step}
            value={safeVal}
            onChange={(e) => setValues((v) => ({ ...v, [param.param_name]: e.target.value }))}
            className="flex-1"
          />
          <span className="w-10 text-right text-xs text-[var(--color-text-secondary)]">
            {values[param.param_name]}
          </span>
        </div>
      );
    }
    case "image_selector":
      return (
        <div className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-xs text-[var(--color-text-muted)]">
          当前选中的图片（自动绑定原图）
        </div>
      );
    default:
      return (
        <input
          type={param.field_type === "number" ? "number" : "text"}
          value={values[param.param_name] ?? param.default_value ?? ""}
          onChange={(e) => setValues((v) => ({ ...v, [param.param_name]: e.target.value }))}
          className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none"
        />
      );
  }
}

export function ComfyUIWorkflowParams({
  params,
  values,
  setValues,
}: {
  params: WorkflowParam[];
  values: Record<string, string>;
  setValues: (fn: (prev: Record<string, string>) => Record<string, string>) => void;
}) {
  return (
    <>
      {params.map((p) => (
        <div key={p.node_id + p.param_name}>
          <label className="mb-1 block text-xs text-[var(--color-text-muted)]">
            #{p.param_name}
          </label>
          <ComfyUIDynamicField param={p} values={values} setValues={setValues} />
        </div>
      ))}
    </>
  );
}
