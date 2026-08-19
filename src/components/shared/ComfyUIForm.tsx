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
      <p
        className="text-xs text-[var(--color-text-muted)] py-3"
        dangerouslySetInnerHTML={{ __html: emptyMessage }}
      />
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

function ControlShell({ param, children }: { param: WorkflowParam; children: React.ReactNode }) {
  return (
    <div>
      <label className="mb-1 block text-xs text-[var(--color-text-muted)]">
        {param.label || param.widget_name}
      </label>
      {children}
      {param.description && (
        <p className="mt-0.5 text-[10px] text-[var(--color-text-muted)]">{param.description}</p>
      )}
    </div>
  );
}

export function ComfyUIDynamicField({ param, values, setValues }: DynamicFieldProps) {
  const raw = values[param.param_name] ?? param.default_value ?? "";

  switch (param.field_type) {
    case "multiline":
      return (
        <ControlShell param={param}>
          <textarea
            value={raw}
            onChange={(e) => setValues((v) => ({ ...v, [param.param_name]: e.target.value }))}
            rows={param.multiline === true ? 6 : 4}
            className="w-full resize-none rounded border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] px-3 py-2 text-sm text-[var(--color-text-primary)] outline-none focus:border-[var(--color-accent)]"
          />
        </ControlShell>
      );
    case "seed":
      return (
        <ControlShell param={param}>
          <div className="flex gap-2">
            <input
              type="number"
              value={raw}
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
        </ControlShell>
      );
    case "slider": {
      const numVal = parseFloat(raw);
      const safeVal = isNaN(numVal) ? (param.min ?? 0) : numVal;
      const min = param.min ?? 0;
      const max = param.max ?? 100;
      const step = param.step ?? 1;
      return (
        <ControlShell param={param}>
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
            <span className="w-12 text-right text-xs text-[var(--color-text-secondary)]">
              {raw}
            </span>
          </div>
        </ControlShell>
      );
    }
    case "number": {
      const min = param.min ?? undefined;
      const max = param.max ?? undefined;
      const step = param.step ?? undefined;
      return (
        <ControlShell param={param}>
          <input
            type="number"
            min={min}
            max={max}
            step={step}
            value={raw}
            onChange={(e) => setValues((v) => ({ ...v, [param.param_name]: e.target.value }))}
            className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none"
          />
        </ControlShell>
      );
    }
    case "combo": {
      const options =
        param.options && param.options.length > 0
          ? param.options
          : raw
            ? [raw]
            : [];
      return (
        <ControlShell param={param}>
          <select
            value={raw}
            onChange={(e) => setValues((v) => ({ ...v, [param.param_name]: e.target.value }))}
            className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none"
          >
            {options.map((o) => (
              <option key={o} value={o}>{o}</option>
            ))}
          </select>
        </ControlShell>
      );
    }
    case "boolean":
      return (
        <ControlShell param={param}>
          <label className="flex items-center gap-2 text-xs text-[var(--color-text-primary)]">
            <input
              type="checkbox"
              checked={raw === "true"}
              onChange={(e) =>
                setValues((v) => ({
                  ...v,
                  [param.param_name]: e.target.checked ? "true" : "false",
                }))
              }
              className="accent-[var(--color-accent)]"
            />
            {raw === "true" ? "开" : "关"}
          </label>
        </ControlShell>
      );
    case "image_selector":
      return (
        <ControlShell param={param}>
          <div className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-xs text-[var(--color-text-muted)]">
            当前选中的图片（自动绑定原图）
          </div>
        </ControlShell>
      );
    default:
      return (
        <ControlShell param={param}>
          <input
            type="text"
            value={raw}
            onChange={(e) => setValues((v) => ({ ...v, [param.param_name]: e.target.value }))}
            className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none"
          />
        </ControlShell>
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
        <ComfyUIDynamicField key={p.param_name} param={p} values={values} setValues={setValues} />
      ))}
    </>
  );
}
