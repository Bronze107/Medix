import { useCallback, useEffect, useRef, useState } from "react";
import { showToast } from "@/components/Toast/Toast";
import { ConfirmDialog } from "@/components/ConfirmDialog/ConfirmDialog";
import { open } from "@tauri-apps/plugin-dialog";
import type { Media } from "@/types/media";
import type { Tag } from "@/types/tag";
import type { Variant, VariantPreset } from "@/types/variant";
import type { Caption } from "@/types/caption";
import {
  mediaTagsGetForVariant,
  mediaTagAddForVariant,
  mediaTagRemoveForVariant,
  tagList,
  tagCreate,
  variantList,
  variantGenerate,
  variantImport,
  variantAnnotate,
  variantPresets,
  mediaSetDisplayVariant,
  captionList,
  captionCreate,
  captionCreateForVariant,
  captionUpdate,
  captionDelete,
  embeddingInfo,
  mediaAiAnnotate,
  aiPendingCount,
  mediaSoftDelete,
} from "@/lib/tauri";
import type { EmbeddingInfo } from "@/types/ai";

interface DetailPanelProps {
  media: Media | null;
  collapsed: boolean;
  onToggleCollapse: () => void;
  onDeleted?: () => void;
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

function DetailPanel({ media, collapsed, onToggleCollapse, onDeleted }: DetailPanelProps) {
  const [activeTab, setActiveTab] = useState<"details" | "captions" | "tags">("details");
  const [showDeleteConfirm, setShowDeleteConfirm] = useState(false);

  // Tags state
  const [tags, setTags] = useState<Tag[]>([]);
  const [allTags, setAllTags] = useState<Tag[]>([]);
  const [newTagInput, setNewTagInput] = useState("");
  const [suggestions, setSuggestions] = useState<Tag[]>([]);
  const [showSuggestions, setShowSuggestions] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  // Version state
  const [variants, setVariants] = useState<Variant[]>([]);
  const [presets, setPresets] = useState<VariantPreset[]>([]);

  // Version generation form
  const [versionLabel, setVersionLabel] = useState("");
  const [versionFormat, setVersionFormat] = useState("jpeg");
  const [versionMaxWidth, setVersionMaxWidth] = useState<number | null>(1080);
  const [versionMaxHeight, setVersionMaxHeight] = useState<number | null>(null);
  const [versionQuality, setVersionQuality] = useState(75);
  const [versionGenerating, setVersionGenerating] = useState(false);

  // Import version
  const [importVersionPath, setImportVersionPath] = useState("");
  const [importingVersion, setImportingVersion] = useState(false);

  // Captions state
  const [captions, setCaptions] = useState<Caption[]>([]);
  const [embeddings, setEmbeddings] = useState<EmbeddingInfo[]>([]);
  const [newCaptionText, setNewCaptionText] = useState("");
  const [editingCaptionId, setEditingCaptionId] = useState<string | null>(null);
  const [editingText, setEditingText] = useState("");
  const [captionVariantId, setCaptionVariantId] = useState<string | null>(null);
  const [targetId, setTargetId] = useState<string | null>(null); // null=original, string=variant_id
  const [showVersionForm, setShowVersionForm] = useState(false);

  const loadAllTags = useCallback(async () => {
    try {
      const list = await tagList();
      setAllTags(list);
    } catch (e) {
      console.error("Failed to load tags:", e);
    }
  }, []);

  const loadMediaTags = useCallback(async (mediaId: string, variantId: string | null) => {
    try {
      const list = await mediaTagsGetForVariant(mediaId, variantId);
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

  const loadCaptions = useCallback(async (mediaId: string) => {
    try {
      const list = await captionList(mediaId);
      setCaptions(list);
    } catch (e) {
      console.error("Failed to load captions:", e);
    }
  }, []);

  const loadEmbeddings = useCallback(async (mediaId: string) => {
    try {
      const list = await embeddingInfo(mediaId);
      setEmbeddings(list);
    } catch (e) {
      console.error("Failed to load embeddings:", e);
    }
  }, []);

  useEffect(() => {
    loadAllTags();
    variantPresets().then(setPresets);
  }, [loadAllTags]);

  useEffect(() => {
    if (media) {
      setTargetId(null);
      setShowVersionForm(false);
      loadMediaTags(media.id, null);
      loadVariants(media.id);
      loadCaptions(media.id);
      loadEmbeddings(media.id);
      setActiveTab("details");
    } else {
      setTags([]);
      setVariants([]);
      setCaptions([]);
      setEmbeddings([]);
      setNewCaptionText("");
      setEditingCaptionId(null);
      setEditingText("");
      setTargetId(null);
    }
  }, [media?.id, loadMediaTags, loadVariants, loadCaptions, loadEmbeddings]);

  // Reload tags when target changes
  useEffect(() => {
    if (media) {
      loadMediaTags(media.id, targetId);
    }
  }, [targetId, media?.id, loadMediaTags]);

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
      await mediaTagAddForVariant(media.id, targetId, tagId);
      await loadMediaTags(media.id, targetId);
      setNewTagInput("");
      setShowSuggestions(false);
    } catch (e) {
      console.error("Failed to add tag:", e);
    }
  };

  const handleRemoveTag = async (tagId: string) => {
    if (!media) return;
    try {
      await mediaTagRemoveForVariant(media.id, targetId, tagId);
      await loadMediaTags(media.id, targetId);
      showToast("已移除标签");
    } catch (e) {
      console.error("Failed to remove tag:", e);
    }
  };

  const fillPreset = (preset: VariantPreset) => {
    setVersionLabel(preset.label);
    setVersionFormat(preset.format);
    setVersionMaxWidth(preset.max_width ?? null);
    setVersionMaxHeight(preset.max_height ?? null);
    setVersionQuality(preset.quality);
  };

  const handleGenerateVersion = async () => {
    if (!media) return;
    setVersionGenerating(true);
    try {
      await variantGenerate(
        media.id,
        versionLabel.trim(),
        versionFormat,
        versionMaxWidth,
        versionMaxHeight,
        versionQuality,
      );
      await loadVariants(media.id);
      setVersionLabel("");
      setShowVersionForm(false);
    } catch (e) {
      console.error("Failed to generate version:", e);
    } finally {
      setVersionGenerating(false);
    }
  };

  const handleImportVersion = async () => {
    if (!media) return;
    const path = importVersionPath.trim();
    if (!path) return;
    setImportingVersion(true);
    try {
      await variantImport(media.id, path);
      await loadVariants(media.id);
      setImportVersionPath("");
      setShowVersionForm(false);
    } catch (e) {
      console.error("Failed to import version:", e);
    } finally {
      setImportingVersion(false);
    }
  };

  const handleAddCaption = async () => {
    if (!media) return;
    const text = newCaptionText.trim();
    if (!text) return;
    try {
      if (captionVariantId) {
        await captionCreateForVariant(media.id, captionVariantId, text);
      } else {
        await captionCreate(media.id, text);
      }
      await loadCaptions(media.id);
      setNewCaptionText("");
    } catch (e) {
      console.error("Failed to add caption:", e);
    }
  };

  const handleStartEdit = (caption: Caption) => {
    setEditingCaptionId(caption.id);
    setEditingText(caption.text);
  };

  const handleSaveEdit = async () => {
    if (!editingCaptionId) return;
    const text = editingText.trim();
    if (!text) return;
    try {
      await captionUpdate(editingCaptionId, text);
      if (media) {
        await loadCaptions(media.id);
      }
      setEditingCaptionId(null);
      setEditingText("");
    } catch (e) {
      console.error("Failed to update caption:", e);
    }
  };

  const handleCancelEdit = () => {
    setEditingCaptionId(null);
    setEditingText("");
  };

  const handleDeleteCaption = async (id: string) => {
    try {
      await captionDelete(id);
      if (media) {
        await loadCaptions(media.id);
      }
      showToast("已删除描述");
    } catch (e) {
      console.error("Failed to delete caption:", e);
    }
  };

  const handleAdoptAiCaption = async (text: string) => {
    if (!media) return;
    try {
      await captionCreate(media.id, text);
      await loadCaptions(media.id);
    } catch (e) {
      console.error("Failed to adopt AI caption:", e);
    }
  };

  if (!media || collapsed) {
    return (
      <div className="flex h-full w-10 flex-col items-center border-l border-[var(--color-border)] bg-[var(--color-bg-secondary)] py-3 transition-all duration-300">
        <button
          onClick={onToggleCollapse}
          className="mb-2 rounded p-0.5 text-[var(--color-text-muted)] hover:bg-[var(--color-bg-hover)] hover:text-[var(--color-text-primary)] transition-colors"
          title={collapsed ? "展开详情" : undefined}
        >
          <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
            <path strokeLinecap="round" strokeLinejoin="round" d="M15.75 19.5 8.25 12l7.5-7.5" />
          </svg>
        </button>
        <p className="text-[10px] text-[var(--color-text-muted)]" style={{ writingMode: "vertical-rl" }}>
          {media ? "详情" : "点击图片查看详情"}
        </p>
      </div>
    );
  }

  return (
    <div className="flex h-full w-80 flex-col border-l border-[var(--color-border)] bg-[var(--color-bg-secondary)] p-4 transition-all duration-300">
      {/* Target selector */}
      <div className="mb-2 flex items-center gap-2">
        <button
          onClick={onToggleCollapse}
          className="rounded p-1 text-[var(--color-text-muted)] hover:bg-[var(--color-bg-hover)] hover:text-[var(--color-text-primary)] transition-colors"
          title="收起详情"
        >
          <svg className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
            <path strokeLinecap="round" strokeLinejoin="round" d="M8.25 4.5l7.5 7.5-7.5 7.5" />
          </svg>
        </button>
        <select
          value={targetId ?? ""}
          onChange={(e) => {
            const val = e.target.value;
            if (val === "__add__") { setShowVersionForm(true); return; }
            setTargetId(val || null);
            setCaptionVariantId(val || null);
          }}
          className="flex-1 rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none"
        >
          <option value="">原图</option>
          {variants.map((v) => (
            <option key={v.id} value={v.id}>
              {v.label || v.preset_name || "未命名版本"}
            </option>
          ))}
          <option disabled>──────────</option>
          <option value="__add__">+ 添加版本...</option>
        </select>
        {media.display_variant_id && (
          targetId === media.display_variant_id ? (
            <span className="text-xs text-[var(--color-accent)]" title="当前显示版本">👁</span>
          ) : targetId ? null : (
            <span className="text-xs text-[var(--color-accent)]" title="当前显示为其他版本">👁</span>
          )
        )}
      </div>

      {/* Tab dots */}
      <div className="mb-3 flex items-center justify-center gap-4 border-b border-[var(--color-border)] pb-2">
        {(["details", "captions", "tags"] as const).map((tab) => {
          const label = tab === "details" ? "详情" : tab === "captions" ? `描述${(() => { const n = captions.filter(c => targetId ? c.variant_id === targetId : !c.variant_id).length; return n > 0 ? ` (${n})` : ""; })()}` : `标签${tags.length > 0 ? ` (${tags.length})` : ""}`;
          const active = activeTab === tab;
          return (
            <button
              key={tab}
              onClick={() => setActiveTab(tab)}
              className={`flex items-center gap-1.5 text-xs transition-colors ${
                active
                  ? "font-semibold text-[var(--color-text-primary)]"
                  : "font-medium text-[var(--color-text-muted)] hover:text-[var(--color-text-secondary)]"
              }`}
            >
              <span className={`inline-block h-1.5 w-1.5 rounded-full ${active ? "bg-[var(--color-accent)]" : "bg-[var(--color-text-muted)]/40"}`}></span>
              {label}
            </button>
          );
        })}
      </div>

      {activeTab === "details" && (
        <div className="flex flex-1 flex-col overflow-hidden">
          <div className="flex-1 overflow-auto">
          <div className="space-y-3 text-sm">
            <div>
              <p className="text-xs text-[var(--color-text-muted)]">ID</p>
              <p className="mt-0.5 break-all font-mono text-xs text-[var(--color-text-secondary)]">
                {media.id}
              </p>
            </div>

            <div>
              <p className="text-xs text-[var(--color-text-muted)]">尺寸</p>
              <p className="mt-0.5 text-[var(--color-text-secondary)]">
                {media.width ?? "?"} × {media.height ?? "?"} px
              </p>
            </div>

            <div>
              <p className="text-xs text-[var(--color-text-muted)]">文件大小</p>
              <p className="mt-0.5 text-[var(--color-text-secondary)]">
                {formatFileSize(media.file_size)}
              </p>
            </div>

            <div>
              <p className="text-xs text-[var(--color-text-muted)]">原始路径</p>
              <p className="mt-0.5 break-all text-xs text-[var(--color-text-secondary)]">
                {media.source_path ?? "—"}
              </p>
            </div>

            {media.source && (
              <div>
                <p className="text-xs text-[var(--color-text-muted)]">来源</p>
                <p className="mt-0.5 text-xs text-[var(--color-text-secondary)]">
                  {media.source === "web" && "网页"}
                  {media.source === "local" && "本地"}
                  {media.source === "zip" && "ZIP 导入"}
                  {media.source !== "web" && media.source !== "local" && media.source !== "zip" && media.source}
                </p>
              </div>
            )}
            {media.source_url && (
              <div>
                <p className="text-xs text-[var(--color-text-muted)]">图片 URL</p>
                <p className="mt-0.5 break-all text-xs">
                  <a
                    href={media.source_url}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="text-[var(--color-accent)] hover:underline"
                  >
                    {media.source_url}
                  </a>
                </p>
              </div>
            )}
            {media.page_url && (
              <div>
                <p className="text-xs text-[var(--color-text-muted)]">页面 URL</p>
                <p className="mt-0.5 break-all text-xs">
                  <a
                    href={media.page_url}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="text-[var(--color-accent)] hover:underline"
                  >
                    {media.page_url}
                  </a>
                </p>
              </div>
            )}

            <div>
              <p className="text-xs text-[var(--color-text-muted)]">创建时间 (EXIF)</p>
              <p className="mt-0.5 text-[var(--color-text-secondary)]">
                {formatDate(media.created_at)}
              </p>
            </div>

            <div>
              <p className="text-xs text-[var(--color-text-muted)]">修改时间 (EXIF)</p>
              <p className="mt-0.5 text-[var(--color-text-secondary)]">
                {formatDate(media.modified_at)}
              </p>
            </div>

            <div>
              <p className="text-xs text-[var(--color-text-muted)]">导入时间</p>
              <p className="mt-0.5 text-[var(--color-text-secondary)]">
                {formatDate(media.imported_at)}
              </p>
            </div>
          </div>

            {/* Embedding status */}
          <div className="mt-6 border-t border-[var(--color-border)] pt-4">
            <p className="mb-2 text-xs text-[var(--color-text-muted)]">向量嵌入</p>
            {embeddings.length === 0 ? (
              <p className="text-xs text-[var(--color-text-muted)]">
                {media ? "暂无 embedding" : "—"}
              </p>
            ) : (
              <div className="space-y-1.5">
                {embeddings.map((e) => (
                  <div
                    key={e.content_type}
                    className="flex items-center justify-between rounded bg-[var(--color-bg-tertiary)]/50 px-2 py-1.5"
                  >
                    <span className="text-xs text-[var(--color-text-secondary)]">
                      {e.content_type === "caption" ? "描述向量" : "标签向量"}
                    </span>
                    <span className="text-[10px] text-[var(--color-text-muted)]">
                      {e.vec_dim}d · {e.model}
                    </span>
                  </div>
                ))}
              </div>
            )}
          </div>
          </div>
        </div>
      )}

      {activeTab === "tags" && (
        <div className="flex flex-1 flex-col overflow-hidden">
          <div className="flex-1 overflow-auto">
            {(() => {
              if (targetId && !variants.some((v) => v.id === targetId)) {
                return <p className="py-4 text-center text-xs text-[var(--color-text-muted)]">版本未找到</p>;
              }
              return (
                <>
                  <div className="mb-2 flex flex-wrap gap-1.5">
                    {tags.length === 0 && (
                      <span className="text-xs text-[var(--color-text-muted)]">暂无标签</span>
                    )}
                    {tags.map((tag) => {
                      const isAi = tag.source === "ai";
                      return (
                        <span
                          key={tag.id}
                          className={`group inline-flex items-center gap-1 rounded-full px-2.5 py-0.5 text-xs ${
                            isAi
                              ? "bg-[var(--color-accent-soft)] text-[var(--color-accent-hover)]"
                              : "bg-[var(--color-bg-tertiary)] text-[var(--color-text-secondary)]"
                          }`}
                        >
                          {tag.name}
                          {isAi && (
                            <span className="rounded bg-[var(--color-accent-soft)] px-1 text-[11px] text-[var(--color-accent)]">
                              AI
                            </span>
                          )}
                          <button
                            onClick={() => handleRemoveTag(tag.id)}
                            className="opacity-0 transition-opacity group-hover:opacity-100"
                            title="移除标签"
                          >
                            <svg className="h-3 w-3 text-[var(--color-text-muted)] hover:text-[var(--color-danger)]" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                              <path strokeLinecap="round" strokeLinejoin="round" d="M6 18 18 6M6 6l12 12" />
                            </svg>
                          </button>
                        </span>
                      );
                    })}
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
                      className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)] focus:border-[var(--color-accent)]"
                    />
                    {showSuggestions && suggestions.length > 0 && (
                      <div className="absolute z-10 mt-1 max-h-40 w-full overflow-auto rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] shadow-lg">
                        {suggestions.map((tag) => (
                          <button
                            key={tag.id}
                            onClick={() => handleAddTag(tag.name)}
                            className="block w-full px-2 py-1.5 text-left text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
                          >
                            {tag.name}
                          </button>
                        ))}
                      </div>
                    )}
                  </div>
                </>
              );
            })()}
          </div>
        </div>
      )}

      {activeTab === "captions" && (
        <div className="flex flex-1 flex-col overflow-hidden">
          <div className="flex-1 overflow-auto">
            {(() => {
              const targetCaptions = captions.filter(
                (c) => targetId ? c.variant_id === targetId : !c.variant_id
              );
              if (targetCaptions.length === 0) {
                return <p className="py-4 text-center text-xs text-[var(--color-text-muted)]">暂无描述</p>;
              }
              return (
              <div className="space-y-2">
                {targetCaptions.map((c) => {
                  const isAi = c.source === "ai";
                  return (
                    <div
                      key={c.id}
                      className={`rounded border p-2.5 ${
                        isAi
                          ? "border-[var(--color-accent)]/30 bg-[var(--color-accent-soft)]"
                          : "border-[var(--color-border)] bg-[var(--color-bg-tertiary)]/50"
                      }`}
                    >
                      {isAi && (
                        <span className="mb-1.5 mr-1 inline-block rounded bg-[var(--color-accent-soft)] px-1.5 py-0.5 text-[10px] font-medium text-[var(--color-accent)]">
                          AI 描述
                        </span>
                      )}
                      {editingCaptionId === c.id ? (
                        <div className="space-y-2">
                          <textarea
                            value={editingText}
                            onChange={(e) => setEditingText(e.target.value)}
                            rows={3}
                            className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)] focus:border-[var(--color-accent)]"
                            autoFocus
                          />
                          <div className="flex gap-2">
                            <button
                              onClick={handleSaveEdit}
                              className="rounded bg-[var(--color-accent)] px-2 py-1 text-xs text-white hover:bg-[var(--color-accent-hover)]"
                            >
                              保存
                            </button>
                            <button
                              onClick={handleCancelEdit}
                              className="rounded border border-[var(--color-border-light)] px-2 py-1 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
                            >
                              取消
                            </button>
                          </div>
                        </div>
                      ) : (
                        <div>
                          <p className={`whitespace-pre-wrap text-xs leading-relaxed ${isAi ? "text-[var(--color-accent-hover)]/80" : "text-[var(--color-text-secondary)]"}`}>
                            {c.text}
                          </p>
                          <div className="mt-2 flex gap-2">
                            {isAi ? (
                              <button
                                onClick={() => handleAdoptAiCaption(c.text)}
                                className="text-[10px] text-[var(--color-accent)] hover:text-[var(--color-accent-hover)]"
                              >
                                采纳为手动描述
                              </button>
                            ) : (
                              <button
                                onClick={() => handleStartEdit(c)}
                                className="text-[10px] text-[var(--color-text-muted)] hover:text-[var(--color-accent)]"
                              >
                                编辑
                              </button>
                            )}
                            <button
                              onClick={() => handleDeleteCaption(c.id)}
                              className="text-[10px] text-[var(--color-text-muted)] hover:text-[var(--color-danger)]"
                            >
                              删除
                            </button>
                          </div>
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            );
            })()}
          </div>

          <div className="mt-4 border-t border-[var(--color-border)] pt-3">
            <p className="mb-2 text-xs text-[var(--color-text-muted)]">添加描述</p>
            <textarea
              value={newCaptionText}
              onChange={(e) => setNewCaptionText(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
                  e.preventDefault();
                  handleAddCaption();
                }
              }}
              rows={3}
              placeholder="输入描述文本... (Ctrl+Enter 保存)"
              className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)] focus:border-[var(--color-accent)]"
            />
            <button
              onClick={handleAddCaption}
              disabled={!newCaptionText.trim()}
              className="mt-2 w-full rounded bg-[var(--color-accent)] px-2 py-1.5 text-xs text-white transition-colors hover:bg-[var(--color-accent-hover)] disabled:opacity-50 disabled:hover:bg-[var(--color-accent)]"
            >
              添加
            </button>
          </div>
        </div>
      )}

      {showVersionForm && (
        <div className="flex flex-col overflow-hidden border-t border-[var(--color-border)] pt-3">
          <div className="mb-2 flex items-center justify-between">
            <p className="text-xs font-semibold text-[var(--color-text-primary)]">添加版本</p>
            <button onClick={() => setShowVersionForm(false)} className="rounded p-0.5 text-[var(--color-text-muted)] hover:text-[var(--color-text-secondary)]">
              <svg className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M6 18 18 6M6 6l12 12" />
              </svg>
            </button>
          </div>

          {/* Import external file as version */}
          <div className="mb-2 flex gap-1.5">
            <input
              type="text"
              value={importVersionPath}
              onChange={(e) => setImportVersionPath(e.target.value)}
              placeholder="外部文件路径..."
              onKeyDown={(e) => { if (e.key === "Enter") handleImportVersion(); }}
              className="flex-1 rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1 text-xs text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)]"
            />
            <button
              onClick={async () => {
                const selected = await open({ multiple: false, filters: [{ name: "图片", extensions: ["jpg", "jpeg", "png", "webp", "gif", "bmp"] }] });
                if (selected) setImportVersionPath(selected);
              }}
              className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
              title="选择文件"
            >...</button>
            <button
              onClick={handleImportVersion}
              disabled={!importVersionPath.trim() || importingVersion}
              className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)] disabled:opacity-50 whitespace-nowrap"
            >{importingVersion ? "导入中..." : "导入"}</button>
          </div>

          {/* Preset templates */}
          <div className="mb-2 flex flex-wrap gap-1">
            {presets.map((p) => (
              <button key={p.name} onClick={() => fillPreset(p)}
                className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-0.5 text-[10px] text-[var(--color-text-muted)] hover:bg-[var(--color-bg-hover)] hover:text-[var(--color-text-secondary)]"
              >{p.label}</button>
            ))}
          </div>

          <div className="space-y-1.5">
            <input type="text" value={versionLabel} onChange={(e) => setVersionLabel(e.target.value)} placeholder="版本名称（可选）"
              className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1 text-xs text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)]" />
            <div className="flex gap-1.5">
              <select value={versionFormat} onChange={(e) => setVersionFormat(e.target.value)}
                className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-1 py-1 text-xs text-[var(--color-text-primary)] outline-none">
                <option value="jpeg">JPEG</option><option value="png">PNG</option>
              </select>
              <input type="number" value={versionMaxWidth ?? ""} onChange={(e) => setVersionMaxWidth(e.target.value ? parseInt(e.target.value) : null)} placeholder="最大宽度"
                className="w-16 rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-1 py-1 text-xs text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)]" />
              <input type="number" value={versionMaxHeight ?? ""} onChange={(e) => setVersionMaxHeight(e.target.value ? parseInt(e.target.value) : null)} placeholder="最大高度"
                className="w-16 rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-1 py-1 text-xs text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)]" />
              <input type="number" value={versionQuality} onChange={(e) => setVersionQuality(parseInt(e.target.value) || 75)} min={1} max={100} placeholder="Q"
                className="w-12 rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-1 py-1 text-xs text-[var(--color-text-primary)] outline-none" title="质量 1-100" />
            </div>
            <button onClick={handleGenerateVersion} disabled={versionGenerating}
              className="w-full rounded bg-[var(--color-accent)] px-2 py-1.5 text-xs text-white transition-colors hover:bg-[var(--color-accent-hover)] disabled:opacity-50"
            >{versionGenerating ? "生成中..." : "生成版本"}</button>
          </div>
        </div>
      )}

      {/* Floating action bar */}
      <div className="mt-auto border-t border-[var(--color-border)] pt-3">
        <div className="flex items-center justify-center gap-2">
          <button
            onClick={async () => {
              if (!media) return;
              try {
                if (targetId) {
                  await variantAnnotate(media.id, targetId);
                } else {
                  await mediaAiAnnotate(media.id);
                }
                let remaining = await aiPendingCount();
                for (let i = 0; i < 10 && remaining > 0; i++) {
                  await new Promise((r) => setTimeout(r, 3000));
                  remaining = await aiPendingCount();
                }
                await loadCaptions(media.id);
                await loadMediaTags(media.id, targetId);
                await loadEmbeddings(media.id);
                showToast("AI 标注完成");
              } catch (e) {
                console.error("Failed to trigger AI annotation:", e);
              }
            }}
            className="rounded-lg p-2 text-[var(--color-text-muted)] hover:bg-[var(--color-accent-soft)] hover:text-[var(--color-accent)] transition-colors"
            title="AI 标注"
          >
            <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M9.813 15.904 9 18.75l-.813-2.846a4.5 4.5 0 0 0-3.09-3.09L2.25 12l2.846-.813a4.5 4.5 0 0 0 3.09-3.09L9 5.25l.813 2.846a4.5 4.5 0 0 0 3.09 3.09L15.75 12l-2.846.813a4.5 4.5 0 0 0-3.09 3.09ZM18.259 8.715 18 9.75l-.259-1.035a3.375 3.375 0 0 0-2.455-2.456L14.25 6l1.036-.259a3.375 3.375 0 0 0 2.455-2.456L18 2.25l.259 1.035a3.375 3.375 0 0 0 2.455 2.456L21.75 6l-1.036.259a3.375 3.375 0 0 0-2.455 2.456ZM16.894 20.567 16.5 21.75l-.394-1.183a2.25 2.25 0 0 0-1.423-1.423L13.5 18.75l1.183-.394a2.25 2.25 0 0 0 1.423-1.423l.394-1.183.394 1.183a2.25 2.25 0 0 0 1.423 1.423l1.183.394-1.183.394a2.25 2.25 0 0 0-1.423 1.423Z" />
            </svg>
          </button>
          {targetId && (
            media.display_variant_id === targetId ? (
              <button
                onClick={async () => {
                  await mediaSetDisplayVariant(media.id, null);
                  window.dispatchEvent(new CustomEvent("display-variant-changed", { detail: { mediaId: media.id, variantId: null } }));
                }}
                className="rounded-lg p-2 text-[var(--color-accent)] hover:bg-[var(--color-bg-hover)] transition-colors"
                title="恢复显示原图"
              >
                <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="M2.036 12.322a1.012 1.012 0 0 1 0-.639C3.423 7.51 7.36 4.5 12 4.5c4.638 0 8.573 3.007 9.963 7.178.07.207.07.431 0 .639C20.577 16.49 16.64 19.5 12 19.5c-4.638 0-8.573-3.007-9.963-7.178Z" />
                  <path strokeLinecap="round" strokeLinejoin="round" d="M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z" />
                </svg>
              </button>
            ) : (
              <button
                onClick={async () => {
                  await mediaSetDisplayVariant(media.id, targetId);
                  window.dispatchEvent(new CustomEvent("display-variant-changed", { detail: { mediaId: media.id, variantId: targetId } }));
                }}
                className="rounded-lg p-2 text-[var(--color-text-muted)] hover:bg-[var(--color-bg-hover)] hover:text-[var(--color-text-primary)] transition-colors"
                title="设为主显示版本"
              >
                <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="M2.036 12.322a1.012 1.012 0 0 1 0-.639C3.423 7.51 7.36 4.5 12 4.5c4.638 0 8.573 3.007 9.963 7.178.07.207.07.431 0 .639C20.577 16.49 16.64 19.5 12 19.5c-4.638 0-8.573-3.007-9.963-7.178Z" />
                  <path strokeLinecap="round" strokeLinejoin="round" d="M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z" />
                </svg>
              </button>
            )
          )}
          <button
            onClick={() => setShowDeleteConfirm(true)}
            className="rounded-lg p-2 text-[var(--color-text-muted)] hover:bg-[var(--color-danger-soft)] hover:text-[var(--color-danger)] transition-colors"
            title="删除图片"
          >
            <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
              <path strokeLinecap="round" strokeLinejoin="round" d="m14.74 9-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 0 1-2.244 2.077H8.084a2.25 2.25 0 0 1-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 0 0-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 0 1 3.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 0 0-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 0 0-7.5 0" />
            </svg>
          </button>
        </div>
      </div>

      <ConfirmDialog
        open={showDeleteConfirm}
        title="删除图片"
        message="确定要删除这张图片吗？可以在回收站中恢复。"
        variant="danger"
        confirmLabel="删除"
        onConfirm={async () => {
          if (!media) return;
          try {
            await mediaSoftDelete(media.id);
            window.dispatchEvent(new CustomEvent("collections-changed"));
            if (onDeleted) onDeleted();
          } catch (e) {
            console.error("Failed to delete:", e);
          } finally {
            setShowDeleteConfirm(false);
          }
        }}
        onCancel={() => setShowDeleteConfirm(false)}
      />
    </div>
  );
}

export default DetailPanel;
