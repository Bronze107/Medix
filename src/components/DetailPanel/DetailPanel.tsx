import { useCallback, useEffect, useRef, useState } from "react";
import type { Media } from "@/types/media";
import type { Tag } from "@/types/tag";
import type { Variant, VariantPreset } from "@/types/variant";
import {
  mediaTagsGet,
  mediaTagAdd,
  mediaTagRemove,
  tagList,
  tagCreate,
  variantList,
  variantGenerate,
  variantDelete,
  variantPresets,
} from "@/lib/tauri";

interface DetailPanelProps {
  media: Media | null;
}

function formatFileSize(bytes: number | null): string {
  if (bytes === null) return "—";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function formatDate(dateStr: string | null): string {
  if (!dateStr) return "—";
  try {
    return new Date(dateStr).toLocaleString("zh-CN");
  } catch {
    return dateStr;
  }
}

function DetailPanel({ media }: DetailPanelProps) {
  const [activeTab, setActiveTab] = useState<"details" | "variants">("details");

  // Tags state
  const [tags, setTags] = useState<Tag[]>([]);
  const [allTags, setAllTags] = useState<Tag[]>([]);
  const [newTagInput, setNewTagInput] = useState("");
  const [suggestions, setSuggestions] = useState<Tag[]>([]);
  const [showSuggestions, setShowSuggestions] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  // Variants state
  const [variants, setVariants] = useState<Variant[]>([]);
  const [presets, setPresets] = useState<VariantPreset[]>([]);
  const [generatingPreset, setGeneratingPreset] = useState<string | null>(null);

  const loadAllTags = useCallback(async () => {
    try {
      const list = await tagList();
      setAllTags(list);
    } catch (e) {
      console.error("Failed to load tags:", e);
    }
  }, []);

  const loadMediaTags = useCallback(async (mediaId: string) => {
    try {
      const list = await mediaTagsGet(mediaId);
      setTags(list);
    } catch (e) {
      console.error("Failed to load media tags:", e);
    }
  }, []);

  const loadVariants = useCallback(async (mediaId: string) => {
    try {
      const list = await variantList(mediaId);
      setVariants(list);
    } catch (e) {
      console.error("Failed to load variants:", e);
    }
  }, []);

  useEffect(() => {
    loadAllTags();
    variantPresets().then(setPresets);
  }, [loadAllTags]);

  useEffect(() => {
    if (media) {
      loadMediaTags(media.id);
      loadVariants(media.id);
      setActiveTab("details");
    } else {
      setTags([]);
      setVariants([]);
    }
  }, [media?.id, loadMediaTags, loadVariants]);

  useEffect(() => {
    const input = newTagInput.trim().toLowerCase();
    if (!input) {
      setSuggestions([]);
      return;
    }
    const filtered = allTags.filter(
      (t) =>
        t.name.toLowerCase().includes(input) &&
        !tags.some((tag) => tag.id === t.id)
    );
    setSuggestions(filtered.slice(0, 8));
  }, [newTagInput, allTags, tags]);

  const handleAddTag = async (tagName: string) => {
    if (!media) return;
    const name = tagName.trim().toLowerCase();
    if (!name) return;

    if (tags.some((t) => t.name.toLowerCase() === name)) {
      setNewTagInput("");
      setShowSuggestions(false);
      return;
    }

    let tagId: string;
    const existing = allTags.find((t) => t.name.toLowerCase() === name);
    if (!existing) {
      try {
        tagId = await tagCreate(name);
        await loadAllTags();
      } catch (e) {
        console.error("Failed to create tag:", e);
        return;
      }
    } else {
      tagId = existing.id;
    }

    try {
      await mediaTagAdd(media.id, tagId);
      await loadMediaTags(media.id);
      setNewTagInput("");
      setShowSuggestions(false);
    } catch (e) {
      console.error("Failed to add tag:", e);
    }
  };

  const handleRemoveTag = async (tagId: string) => {
    if (!media) return;
    try {
      await mediaTagRemove(media.id, tagId);
      await loadMediaTags(media.id);
    } catch (e) {
      console.error("Failed to remove tag:", e);
    }
  };

  const handleGenerateVariant = async (presetName: string) => {
    if (!media) return;
    setGeneratingPreset(presetName);
    try {
      await variantGenerate(media.id, presetName);
      await loadVariants(media.id);
    } catch (e) {
      console.error("Failed to generate variant:", e);
    } finally {
      setGeneratingPreset(null);
    }
  };

  const handleDeleteVariant = async (variantId: string) => {
    try {
      await variantDelete(variantId);
      if (media) {
        await loadVariants(media.id);
      }
    } catch (e) {
      console.error("Failed to delete variant:", e);
    }
  };

  if (!media) {
    return (
      <div className="flex h-full w-72 flex-col border-l border-neutral-800 bg-neutral-900 p-4">
        <p className="text-sm text-neutral-500">选择一张图片查看详情</p>
      </div>
    );
  }

  return (
    <div className="flex h-full w-72 flex-col border-l border-neutral-800 bg-neutral-900 p-4">
      {/* Tabs */}
      <div className="mb-4 flex border-b border-neutral-800">
        <button
          onClick={() => setActiveTab("details")}
          className={`px-3 py-1.5 text-xs font-medium transition-colors ${
            activeTab === "details"
              ? "border-b-2 border-blue-500 text-blue-400"
              : "text-neutral-500 hover:text-neutral-300"
          }`}
        >
          详情
        </button>
        <button
          onClick={() => setActiveTab("variants")}
          className={`px-3 py-1.5 text-xs font-medium transition-colors ${
            activeTab === "variants"
              ? "border-b-2 border-blue-500 text-blue-400"
              : "text-neutral-500 hover:text-neutral-300"
          }`}
        >
          变体 {variants.length > 0 && `(${variants.length})`}
        </button>
      </div>

      {activeTab === "details" && (
        <>
          <div className="space-y-3 text-sm">
            <div>
              <p className="text-xs text-neutral-500">ID</p>
              <p className="mt-0.5 break-all font-mono text-xs text-neutral-300">
                {media.id}
              </p>
            </div>

            <div>
              <p className="text-xs text-neutral-500">尺寸</p>
              <p className="mt-0.5 text-neutral-300">
                {media.width ?? "?"} × {media.height ?? "?"} px
              </p>
            </div>

            <div>
              <p className="text-xs text-neutral-500">文件大小</p>
              <p className="mt-0.5 text-neutral-300">
                {formatFileSize(media.file_size)}
              </p>
            </div>

            <div>
              <p className="text-xs text-neutral-500">原始路径</p>
              <p className="mt-0.5 break-all text-xs text-neutral-400">
                {media.source_path ?? "—"}
              </p>
            </div>

            <div>
              <p className="text-xs text-neutral-500">创建时间 (EXIF)</p>
              <p className="mt-0.5 text-neutral-300">
                {formatDate(media.created_at)}
              </p>
            </div>

            <div>
              <p className="text-xs text-neutral-500">修改时间 (EXIF)</p>
              <p className="mt-0.5 text-neutral-300">
                {formatDate(media.modified_at)}
              </p>
            </div>

            <div>
              <p className="text-xs text-neutral-500">导入时间</p>
              <p className="mt-0.5 text-neutral-300">
                {formatDate(media.imported_at)}
              </p>
            </div>
          </div>

          {/* Tags section */}
          <div className="mt-6 border-t border-neutral-800 pt-4">
            <p className="mb-2 text-xs text-neutral-500">标签</p>

            <div className="mb-2 flex flex-wrap gap-1.5">
              {tags.length === 0 && (
                <span className="text-xs text-neutral-600">暂无标签</span>
              )}
              {tags.map((tag) => (
                <span
                  key={tag.id}
                  className="group inline-flex items-center gap-1 rounded-full bg-neutral-800 px-2.5 py-0.5 text-xs text-neutral-300"
                >
                  {tag.name}
                  <button
                    onClick={() => handleRemoveTag(tag.id)}
                    className="opacity-0 transition-opacity group-hover:opacity-100"
                    title="移除标签"
                  >
                    <svg
                      className="h-3 w-3 text-neutral-500 hover:text-red-400"
                      fill="none"
                      viewBox="0 0 24 24"
                      stroke="currentColor"
                      strokeWidth={2}
                    >
                      <path
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        d="M6 18 18 6M6 6l12 12"
                      />
                    </svg>
                  </button>
                </span>
              ))}
            </div>

            <div className="relative">
              <input
                ref={inputRef}
                type="text"
                value={newTagInput}
                onChange={(e) => {
                  setNewTagInput(e.target.value);
                  setShowSuggestions(true);
                }}
                onFocus={() => setShowSuggestions(true)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    handleAddTag(newTagInput);
                  }
                  if (e.key === "Escape") {
                    setShowSuggestions(false);
                  }
                }}
                placeholder="添加标签..."
                className="w-full rounded border border-neutral-700 bg-neutral-800 px-2 py-1.5 text-xs text-neutral-200 outline-none placeholder:text-neutral-500 focus:border-blue-500"
              />
              {showSuggestions && suggestions.length > 0 && (
                <div className="absolute z-10 mt-1 max-h-40 w-full overflow-auto rounded border border-neutral-700 bg-neutral-800 shadow-lg">
                  {suggestions.map((tag) => (
                    <button
                      key={tag.id}
                      onClick={() => handleAddTag(tag.name)}
                      className="block w-full px-2 py-1.5 text-left text-xs text-neutral-300 hover:bg-neutral-700"
                    >
                      {tag.name}
                    </button>
                  ))}
                </div>
              )}
            </div>
          </div>
        </>
      )}

      {activeTab === "variants" && (
        <div className="flex flex-1 flex-col overflow-hidden">
          <div className="flex-1 overflow-auto">
            {variants.length === 0 && (
              <p className="py-4 text-center text-xs text-neutral-600">暂无变体</p>
            )}
            <div className="space-y-2">
              {variants.map((v) => {
                const preset = presets.find((p) => p.name === v.preset_name);
                return (
                  <div
                    key={v.id}
                    className="rounded border border-neutral-800 bg-neutral-800/50 p-2.5"
                  >
                    <div className="flex items-center justify-between">
                      <span className="text-xs font-medium text-neutral-300">
                        {preset?.label ?? v.preset_name}
                      </span>
                      <button
                        onClick={() => handleDeleteVariant(v.id)}
                        className="rounded p-1 text-neutral-500 transition-colors hover:bg-red-900/30 hover:text-red-400"
                        title="删除变体"
                      >
                        <svg
                          className="h-3.5 w-3.5"
                          fill="none"
                          viewBox="0 0 24 24"
                          stroke="currentColor"
                          strokeWidth={2}
                        >
                          <path
                            strokeLinecap="round"
                            strokeLinejoin="round"
                            d="m14.74 9-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 0 1-2.244 2.077H8.084a2.25 2.25 0 0 1-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 0 0-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 0 1 3.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 0 0-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 0 0-7.5 0"
                          />
                        </svg>
                      </button>
                    </div>
                    <p className="mt-1 text-[10px] text-neutral-500">
                      {v.format.toUpperCase()} · {v.width ?? "?"}×{v.height ?? "?"} ·{" "}
                      {formatFileSize(v.file_size)}
                      {v.quality && v.format === "jpeg" && ` · Q${v.quality}`}
                    </p>
                  </div>
                );
              })}
            </div>
          </div>

          <div className="mt-4 border-t border-neutral-800 pt-3">
            <p className="mb-2 text-xs text-neutral-500">生成变体</p>
            <div className="flex flex-wrap gap-2">
              {presets.map((p) => {
                const exists = variants.some((v) => v.preset_name === p.name);
                const isGenerating = generatingPreset === p.name;
                return (
                  <button
                    key={p.name}
                    onClick={() => handleGenerateVariant(p.name)}
                    disabled={exists || isGenerating}
                    className={`rounded border px-2 py-1 text-xs transition-colors ${
                      exists
                        ? "border-green-900/50 bg-green-900/20 text-green-400"
                        : isGenerating
                        ? "border-neutral-700 bg-neutral-800 text-neutral-500"
                        : "border-neutral-700 bg-neutral-800 text-neutral-300 hover:bg-neutral-700"
                    }`}
                  >
                    {p.label}
                    {isGenerating && "..."}
                    {exists && " ✓"}
                  </button>
                );
              })}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export default DetailPanel;
