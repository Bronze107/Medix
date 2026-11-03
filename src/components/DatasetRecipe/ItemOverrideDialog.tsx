import { useEffect, useState } from "react";
import {
  datasetItemOverridesList,
  datasetItemOverrideSet,
  datasetRecipeGet,
} from "@/lib/tauri";
import type { DatasetRecipe } from "@/types/dataset";

interface ItemOverrideDialogProps {
  collectionId: string;
  mediaId: string;
  onClose: () => void;
}

const CAPTION_OPTIONS: { value: "all" | "manual" | "ai" | "latest"; label: string }[] = [
  { value: "all", label: "全部" },
  { value: "latest", label: "最新一条" },
  { value: "manual", label: "仅手动" },
  { value: "ai", label: "仅 AI" },
];

export function ItemOverrideDialog({ collectionId, mediaId, onClose }: ItemOverrideDialogProps) {
  const [recipe, setRecipe] = useState<DatasetRecipe | null>(null);
  const [captionMode, setCaptionMode] = useState<"all" | "manual" | "ai" | "latest" | "">("");
  /** 每条规则是否启用；未覆盖时全部勾选。 */
  const [checked, setChecked] = useState<Record<string, boolean>>({});
  const [hasOverride, setHasOverride] = useState(false);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    Promise.all([
      datasetRecipeGet(collectionId),
      datasetItemOverridesList(collectionId),
    ])
      .then(([r, overrides]) => {
        if (cancelled) return;
        setRecipe(r);
        const override = overrides.find((o) => o.media_id === mediaId) ?? null;
        if (!r) return;
        if (override) {
          setHasOverride(true);
          if (override.caption_mode) setCaptionMode(override.caption_mode);
        }
        const map: Record<string, boolean> = {};
        for (const rule of r.rules) {
          map[rule.id] = override?.enabled_rules
            ? override.enabled_rules.includes(rule.id)
            : true;
        }
        setChecked(map);
      })
      .catch((e) => setError(String(e)))
      .finally(() => !cancelled && setLoading(false));
    return () => {
      cancelled = true;
    };
  }, [collectionId, mediaId]);

  const handleSave = async () => {
    if (!recipe) return;
    setSaving(true);
    setError(null);
    try {
      const enabledIds = recipe.rules.filter((r) => checked[r.id] !== false).map((r) => r.id);
      const allEnabled = enabledIds.length === recipe.rules.length;
      await datasetItemOverrideSet(
        collectionId,
        mediaId,
        captionMode || null, // 空值 = 跟随配方
        allEnabled ? null : enabledIds // 全勾 = 跟随配方；部分/全不勾 = 覆盖列表
      );
      window.dispatchEvent(new CustomEvent("dataset-recipe-changed"));
      onClose();
    } catch (e) {
      setError(String(e));
      setSaving(false);
    }
  };

  const handleClear = async () => {
    setSaving(true);
    try {
      await datasetItemOverrideSet(collectionId, mediaId, null, null);
      window.dispatchEvent(new CustomEvent("dataset-recipe-changed"));
      onClose();
    } catch (e) {
      setError(String(e));
      setSaving(false);
    }
  };

  const rules = recipe?.rules ?? [];

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-[var(--color-bg-overlay)] animate-fade-in"
      onClick={onClose}
    >
      <div
        className="w-80 rounded-xl border border-[var(--color-border)] bg-[var(--color-bg-elevated)] p-5 shadow-2xl animate-scale-in"
        onClick={(e) => e.stopPropagation()}
      >
        <h3 className="mb-1 text-sm font-semibold text-[var(--color-text-primary)]">
          数据集覆盖设置
        </h3>
        <p className="mb-4 text-[11px] text-[var(--color-text-muted)]">
          为这一张单独设置 caption 与规格规则；留空表示跟随配方。
        </p>

        {loading ? (
          <p className="py-6 text-center text-xs text-[var(--color-text-muted)]">加载中…</p>
        ) : (
          <div className="space-y-4">
            <div>
              <label className="mb-1.5 block text-xs text-[var(--color-text-secondary)]">
                Caption 来源
              </label>
              <select
                value={captionMode}
                onChange={(e) => setCaptionMode(e.target.value as typeof captionMode)}
                className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none focus:border-[var(--color-accent)]"
              >
                <option value="">跟随配方</option>
                {CAPTION_OPTIONS.map((o) => (
                  <option key={o.value} value={o.value}>
                    {o.label}
                  </option>
                ))}
              </select>
            </div>

            <div>
              <label className="mb-1.5 block text-xs text-[var(--color-text-secondary)]">
                规格规则
              </label>
              {rules.length === 0 ? (
                <p className="text-[11px] text-[var(--color-text-muted)]">
                  配方未设置规则
                </p>
              ) : (
                <div className="space-y-1">
                  {rules.map((rule) => (
                    <label
                      key={rule.id}
                      className="flex items-center gap-2 text-xs text-[var(--color-text-secondary)]"
                    >
                      <input
                        type="checkbox"
                        checked={checked[rule.id] !== false}
                        onChange={(e) =>
                          setChecked((c) => ({ ...c, [rule.id]: e.target.checked }))
                        }
                      />
                      {rule.label || `规则 ${rule.position + 1}`}
                      <span className="text-[11px] text-[var(--color-text-muted)]">
                        ({rule.format === "png" ? "PNG" : `JPEG · 质量 ${rule.quality}`})
                      </span>
                    </label>
                  ))}
                </div>
              )}
            </div>

            {error && (
              <p className="rounded bg-[var(--color-danger-soft)] px-3 py-2 text-xs text-[var(--color-danger)]">
                {error}
              </p>
            )}
          </div>
        )}

        <div className="mt-5 flex justify-between gap-2">
          <button
            onClick={handleClear}
            disabled={loading || saving || !hasOverride}
            className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] px-3 py-1.5 text-xs text-[var(--color-text-secondary)] transition-colors hover:bg-[var(--color-bg-hover)] disabled:opacity-40 active:scale-[0.97]"
          >
            恢复跟随配方
          </button>
          <div className="flex gap-2">
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
              {saving ? "保存中…" : "保存"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
