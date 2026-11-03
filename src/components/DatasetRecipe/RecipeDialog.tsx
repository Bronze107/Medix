import { useEffect, useState } from "react";
import {
  datasetRecipeGet,
  datasetRecipeSave,
} from "@/lib/tauri";
import type {
  DatasetRecipe,
  DatasetRecipeInput,
  DatasetRuleInput,
  ResizeFilter,
} from "@/types/dataset";

interface RecipeDialogProps {
  collectionId: string;
  onClose: () => void;
}

/** 本地编辑形态：未保存的规则没有真实 id，用临时 key。 */
interface EditableRule extends DatasetRuleInput {
  _key: string;
}

const CAPTION_OPTIONS: { value: DatasetRecipeInput["caption_mode"]; label: string }[] = [
  { value: "all", label: "全部" },
  { value: "latest", label: "最新一条" },
  { value: "manual", label: "仅手动" },
  { value: "ai", label: "仅 AI" },
];

const FILTER_OPTIONS: { value: ResizeFilter; label: string }[] = [
  { value: "triangle", label: "Triangle（默认）" },
  { value: "nearest", label: "Nearest（最快）" },
  { value: "catmullrom", label: "CatmullRom" },
  { value: "gaussian", label: "Gaussian" },
  { value: "lanczos3", label: "Lanczos3（最清晰）" },
];

let ruleKeySeq = 0;
function newRule(): EditableRule {
  ruleKeySeq += 1;
  return {
    _key: `rule_${Date.now()}_${ruleKeySeq}`,
    label: null,
    format: "jpeg",
    quality: 85,
    max_megapixels: 1.0,
    resize_filter: "triangle",
  };
}

export function RecipeDialog({ collectionId, onClose }: RecipeDialogProps) {
  const [captionMode, setCaptionMode] = useState<DatasetRecipeInput["caption_mode"]>("all");
  const [namingTemplate, setNamingTemplate] = useState("{stem}");
  const [rules, setRules] = useState<EditableRule[]>([]);
  const [hasRecipe, setHasRecipe] = useState(false);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    datasetRecipeGet(collectionId)
      .then((recipe: DatasetRecipe | null) => {
        if (cancelled) return;
        if (recipe) {
          setCaptionMode(recipe.caption_mode);
          setNamingTemplate(recipe.naming_template);
          setRules(
            recipe.rules.map((r) => ({
              _key: r.id,
              label: r.label,
              format: r.format,
              quality: r.quality,
              max_megapixels: r.max_megapixels,
              resize_filter: r.resize_filter,
            }))
          );
          setHasRecipe(true);
        }
      })
      .catch((e) => setError(String(e)))
      .finally(() => !cancelled && setLoading(false));
    return () => {
      cancelled = true;
    };
  }, [collectionId]);

  const updateRule = (key: string, patch: Partial<EditableRule>) => {
    setRules((rs) => rs.map((r) => (r._key === key ? { ...r, ...patch } : r)));
  };

  const moveRule = (idx: number, dir: -1 | 1) => {
    setRules((rs) => {
      const next = [...rs];
      const target = idx + dir;
      if (target < 0 || target >= next.length) return rs;
      [next[idx], next[target]] = [next[target], next[idx]];
      return next;
    });
  };

  const handleSave = async () => {
    setSaving(true);
    setError(null);
    try {
      await datasetRecipeSave(collectionId, {
        caption_mode: captionMode,
        naming_template: namingTemplate.trim() || "{stem}",
        rules: rules.map(({ _key, ...r }) => r),
      });
      window.dispatchEvent(new CustomEvent("dataset-recipe-changed"));
      onClose();
    } catch (e) {
      setError(String(e));
      setSaving(false);
    }
  };

  const inputCls =
    "rounded border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] px-2 py-1 text-xs text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)] focus:border-[var(--color-accent)]";

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-[var(--color-bg-overlay)] animate-fade-in"
      onClick={onClose}
    >
      <div
        className="flex max-h-[85vh] w-[560px] flex-col rounded-xl border border-[var(--color-border)] bg-[var(--color-bg-elevated)] p-5 shadow-2xl animate-scale-in"
        onClick={(e) => e.stopPropagation()}
      >
        <h3 className="mb-1 text-sm font-semibold text-[var(--color-text-primary)]">
          导出配方{hasRecipe ? "" : "（未设置）"}
        </h3>
        <p className="mb-4 text-[11px] text-[var(--color-text-muted)]">
          配方决定数据集导出时的 caption 来源与图片规格；导出时可在导出对话框选择是否应用。
        </p>

        {loading ? (
          <p className="py-8 text-center text-xs text-[var(--color-text-muted)]">加载中…</p>
        ) : (
          <div className="min-h-0 flex-1 space-y-4 overflow-y-auto pr-1">
            {/* Caption 来源 */}
            <div>
              <label className="mb-1.5 block text-xs text-[var(--color-text-secondary)]">
                Caption 来源
              </label>
              <select
                value={captionMode}
                onChange={(e) => setCaptionMode(e.target.value as DatasetRecipeInput["caption_mode"])}
                className={`${inputCls} w-full`}
              >
                {CAPTION_OPTIONS.map((o) => (
                  <option key={o.value} value={o.value}>
                    {o.label}
                  </option>
                ))}
              </select>
            </div>

            {/* 命名模板 */}
            <div>
              <label className="mb-1.5 block text-xs text-[var(--color-text-secondary)]">
                命名模板
              </label>
              <input
                type="text"
                value={namingTemplate}
                onChange={(e) => setNamingTemplate(e.target.value)}
                placeholder="{stem}"
                className={`${inputCls} w-full font-mono`}
              />
              <p className="mt-1 text-[11px] text-[var(--color-text-muted)]">
                占位符：{"{stem}"} 源文件名 · {"{index}"} 序号 · {"{rule}"} 规则名 · {"{w}"}/{"{h}"} 尺寸 · {"{date}"} 日期
              </p>
            </div>

            {/* 规格规则 */}
            <div>
              <div className="mb-1.5 flex items-center justify-between">
                <label className="text-xs text-[var(--color-text-secondary)]">
                  规格规则（每张图片按规则生成对应产物）
                </label>
                <button
                  onClick={() => setRules((rs) => [...rs, newRule()])}
                  className="rounded bg-[var(--color-accent)] px-2 py-1 text-[11px] font-medium text-white transition-colors hover:bg-[var(--color-accent-hover)] active:scale-[0.97]"
                >
                  添加规则
                </button>
              </div>

              {rules.length === 0 && (
                <p className="rounded-lg border border-dashed border-[var(--color-border-light)] px-3 py-4 text-center text-[11px] text-[var(--color-text-muted)]">
                  无规则时按原图导出
                </p>
              )}

              <div className="space-y-2">
                {rules.map((rule, idx) => (
                  <div
                    key={rule._key}
                    className="rounded-lg border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] p-2.5"
                  >
                    <div className="mb-2 flex items-center gap-1.5">
                      <span className="text-[11px] tabular-nums text-[var(--color-text-muted)]">
                        #{idx + 1}
                      </span>
                      <input
                        type="text"
                        value={rule.label ?? ""}
                        onChange={(e) => updateRule(rule._key, { label: e.target.value || null })}
                        placeholder="规则名（用于 {rule}）"
                        className={`${inputCls} flex-1`}
                      />
                      <button
                        onClick={() => moveRule(idx, -1)}
                        disabled={idx === 0}
                        className="rounded px-1 text-xs text-[var(--color-text-muted)] transition-colors hover:bg-[var(--color-bg-hover)] hover:text-[var(--color-text-primary)] disabled:opacity-30"
                        title="上移"
                      >
                        ↑
                      </button>
                      <button
                        onClick={() => moveRule(idx, 1)}
                        disabled={idx === rules.length - 1}
                        className="rounded px-1 text-xs text-[var(--color-text-muted)] transition-colors hover:bg-[var(--color-bg-hover)] hover:text-[var(--color-text-primary)] disabled:opacity-30"
                        title="下移"
                      >
                        ↓
                      </button>
                      <button
                        onClick={() => setRules((rs) => rs.filter((r) => r._key !== rule._key))}
                        className="rounded px-1 text-xs text-[var(--color-text-muted)] transition-colors hover:bg-[var(--color-danger-soft)] hover:text-[var(--color-danger)]"
                        title="删除规则"
                      >
                        ✕
                      </button>
                    </div>
                    <div className="grid grid-cols-2 gap-2">
                      <div className="flex items-center gap-1.5">
                        <span className="w-9 text-[11px] text-[var(--color-text-muted)]">格式</span>
                        <select
                          value={rule.format}
                          onChange={(e) =>
                            updateRule(rule._key, { format: e.target.value as "jpeg" | "png" })
                          }
                          className={`${inputCls} flex-1`}
                        >
                          <option value="jpeg">JPEG</option>
                          <option value="png">PNG</option>
                        </select>
                      </div>
                      <div className="flex items-center gap-1.5">
                        <span className="w-9 text-[11px] text-[var(--color-text-muted)]">质量</span>
                        <input
                          type="number"
                          min={1}
                          max={100}
                          value={rule.quality}
                          onChange={(e) => updateRule(rule._key, { quality: Number(e.target.value) })}
                          disabled={rule.format === "png"}
                          className={`${inputCls} flex-1 disabled:opacity-40`}
                        />
                      </div>
                      <div className="flex items-center gap-1.5">
                        <span className="w-16 text-[11px] text-[var(--color-text-muted)]">
                          最大MP
                        </span>
                        <input
                          type="number"
                          min={0.01}
                          step={0.1}
                          value={rule.max_megapixels ?? ""}
                          onChange={(e) =>
                            updateRule(rule._key, {
                              max_megapixels: e.target.value ? Number(e.target.value) : null,
                            })
                          }
                          placeholder="不限"
                          className={`${inputCls} flex-1`}
                        />
                      </div>
                      <div className="flex items-center gap-1.5">
                        <span className="w-9 text-[11px] text-[var(--color-text-muted)]">滤镜</span>
                        <select
                          value={rule.resize_filter ?? "triangle"}
                          onChange={(e) =>
                            updateRule(rule._key, { resize_filter: e.target.value as ResizeFilter })
                          }
                          className={`${inputCls} flex-1`}
                        >
                          {FILTER_OPTIONS.map((o) => (
                            <option key={o.value} value={o.value}>
                              {o.label}
                            </option>
                          ))}
                        </select>
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            </div>

            {error && (
              <p className="rounded bg-[var(--color-danger-soft)] px-3 py-2 text-xs text-[var(--color-danger)]">
                {error}
              </p>
            )}
          </div>
        )}

        <div className="mt-4 flex justify-end gap-2 border-t border-[var(--color-border-light)] pt-4">
          <button
            onClick={onClose}
            className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] px-3 py-1.5 text-xs text-[var(--color-text-secondary)] transition-colors hover:bg-[var(--color-bg-hover)] active:scale-[0.97]"
          >
            取消
          </button>
          <button
            onClick={handleSave}
            disabled={loading || saving}
            className="rounded bg-[var(--color-accent)] px-4 py-1.5 text-xs font-medium text-white transition-colors hover:bg-[var(--color-accent-hover)] disabled:opacity-50 active:scale-[0.97]"
          >
            {saving ? "保存中…" : "保存配方"}
          </button>
        </div>
      </div>
    </div>
  );
}
