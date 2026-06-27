import { useCallback, useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";

function formatDurationChinese(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = Math.floor(seconds % 60);
  if (h > 0) return `${h}时${m}分${s}秒`;
  if (m > 0 && s > 0) return `${m}分${s}秒`;
  if (m > 0) return `${m}分`;
  return `${s}秒`;
}
import { listen } from "@tauri-apps/api/event";
import { showToast } from "@/components/Toast/Toast";
import { ConfirmDialog } from "@/components/ConfirmDialog/ConfirmDialog";
import { open } from "@tauri-apps/plugin-dialog";
import { useThumbnail } from "@/hooks/useThumbnail";
import ImagineDialog from "@/components/ImagineDialog/ImagineDialog";
import ExportDialog from "@/components/ExportDialog/ExportDialog";
import type { Media } from "@/types/media";
import type { Tag } from "@/types/tag";
import type { Variant, VariantPreset } from "@/types/variant";
import type { Caption } from "@/types/caption";
import {
  mediaTagsGetForVariant,
  mediaTagAddForVariant,
  mediaTagRemoveForVariant,
  mediaTagsClear,
  tagList,
  tagCreate,
  variantList,
  variantGenerate,
  variantImport,
  variantDelete,
  variantAnnotate,
  variantPresets,
  mediaSetDisplayVariant,
  captionList,
  captionCreate,
  captionCreateForVariant,
  captionUpdate,
  captionDelete,
  embeddingInfo,
  embeddingDelete,
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
  initialVariantId?: string | null;
}

function parsePlatform(url: string | null): string | null {
  if (!url) return null;
  try {
    const host = new URL(url).hostname.replace("www.", "");
    const map: Record<string, string> = {
      // Social platforms
      "xiaohongshu.com": "小红书",
      "xhs.sh": "小红书",
      "weibo.com": "微博",
      "sinaimg.cn": "微博",
      "douyin.com": "抖音",
      "bilibili.com": "B站",
      "twitter.com": "Twitter",
      "x.com": "Twitter",
      "twimg.com": "Twitter",
      "instagram.com": "Instagram",
      "cdninstagram.com": "Instagram",
      "zhihu.com": "知乎",
      "lofter.com": "LOFTER",
      "tumblr.com": "Tumblr",
      "deviantart.com": "DeviantArt",
      "artstation.com": "ArtStation",
      "pixiv.net": "Pixiv",
      "pximg.net": "Pixiv",
      "reddit.com": "Reddit",
      "flickr.com": "Flickr",
      "pinterest.com": "Pinterest",
      "pinimg.com": "Pinterest",
      "facebook.com": "Facebook",
      "fbcdn.net": "Facebook",
      "discord.com": "Discord",
      "discordapp.net": "Discord",
      // Image hosts
      "imgur.com": "Imgur",
      "gyazo.com": "Gyazo",
      "deviantart.net": "DeviantArt",
    };
    for (const [domain, name] of Object.entries(map)) {
      if (host === domain || host.endsWith("." + domain)) return name;
    }
    return host;
  } catch {
    return null;
  }
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

// --- Dropdown for variant list, kept in DOM (hidden when closed) for image cache ---
function TargetMenu({
  media,
  variants,
  targetId,
  onSelect,
  onAdd,
}: {
  media: Media;
  variants: Variant[];
  targetId: string | null;
  onSelect: (id: string | null) => void;
  onAdd: () => void;
}) {
  return (
    <div className="absolute left-0 right-0 top-full z-20 mt-1 max-h-64 overflow-auto rounded-lg border border-[var(--color-border)] bg-[var(--color-bg-elevated)] shadow-lg">
      {/* Original */}
      <button
        onClick={() => onSelect(null)}
        className={`flex w-full items-center gap-2 px-3 py-2 text-left transition-colors hover:bg-[var(--color-bg-hover)] ${!targetId ? "bg-[var(--color-accent-soft)]" : ""}`}
      >
        <div className="h-9 w-9 shrink-0 overflow-hidden rounded bg-[var(--color-bg-tertiary)]">
          {media.thumb_256 ? (
            <img src={convertFileSrc(media.thumb_256)} alt="" className="h-full w-full object-cover" draggable={false} decoding="async" />
          ) : (
            <div className="h-full w-full bg-[var(--color-bg-secondary)]" />
          )}
        </div>
        <span className={`text-xs ${!targetId ? "font-semibold text-[var(--color-accent)]" : "text-[var(--color-text-secondary)]"}`}>原图</span>
      </button>
      {variants.map((v) => {
        const label = v.label || v.preset_name || "未命名版本";
        const active = targetId === v.id;
        const fmt = v.format.toUpperCase();
        const dim = `${v.width ?? "?"}×${v.height ?? "?"}`;
        return (
          <button
            key={v.id}
            onClick={() => onSelect(v.id)}
            className={`flex w-full items-center gap-2 px-3 py-2 text-left transition-colors hover:bg-[var(--color-bg-hover)] ${active ? "bg-[var(--color-accent-soft)]" : ""}`}
          >
            <div className="h-9 w-9 shrink-0 overflow-hidden rounded bg-[var(--color-bg-tertiary)]">
              <MenuThumb itemId={v.id} filePath={v.file_path} />
            </div>
            <div className="min-w-0 flex-1 text-left">
              <div className={`truncate text-xs ${active ? "font-semibold text-[var(--color-accent)]" : "text-[var(--color-text-secondary)]"}`}>
                {label}
                {media.display_variant_id === v.id && (
                  <span className="ml-1 text-[10px] text-[var(--color-accent)]">👁</span>
                )}
              </div>
              <p className="truncate text-[10px] text-[var(--color-text-muted)]">
                {fmt}{v.quality && v.format === "jpeg" ? `·Q${v.quality}` : ""} · {dim} · {formatFileSize(v.file_size)}
              </p>
            </div>
          </button>
        );
      })}
      <div className="border-t border-[var(--color-border)]">
        <button
          onClick={onAdd}
          className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-[var(--color-text-muted)] transition-colors hover:bg-[var(--color-bg-hover)] hover:text-[var(--color-text-secondary)]"
        >
          + 添加版本...
        </button>
      </div>
    </div>
  );
}

function MenuThumb({ itemId, filePath }: { itemId: string; filePath?: string }) {
  const thumbUrl = useThumbnail(itemId, undefined);
  // Prefer thumbnail over full-size variant image
  const url = thumbUrl || (filePath ? convertFileSrc(filePath) : null);
  const [loaded, setLoaded] = useState(false);

  if (!url) return <div className="h-full w-full bg-[var(--color-bg-secondary)]" />;

  return (
    <img
      src={url}
      alt=""
      decoding="async"
      loading="lazy"
      className={`h-full w-full object-cover transition-opacity duration-200 ${loaded ? "opacity-100" : "opacity-0"}`}
      onLoad={() => setLoaded(true)}
    />
  );
}

function ThumbnailPreview({
  media,
  targetId,
  variants,
}: {
  media: Media;
  targetId: string | null;
  variants: Variant[];
}) {
  const [loaded, setLoaded] = useState(false);
  const variantThumbUrl = useThumbnail(targetId ?? "", undefined);
  let thumbUrl: string | null = null;

  if (targetId) {
    thumbUrl = variantThumbUrl || null;
    if (!thumbUrl) {
      const variant = variants.find((v) => v.id === targetId);
      if (variant?.file_path) {
        thumbUrl = convertFileSrc(variant.file_path);
      }
    }
  } else {
    thumbUrl = media.thumb_256 ? convertFileSrc(media.thumb_256) : null;
  }

  if (!thumbUrl) {
    return (
      <div className="mb-3 flex h-28 items-center justify-center overflow-hidden rounded-lg bg-[var(--color-bg-tertiary)]">
        <svg className="h-8 w-8 text-[var(--color-text-muted)]" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1}>
          <path strokeLinecap="round" strokeLinejoin="round" d="m2.25 15.75 5.159-5.159a2.25 2.25 0 0 1 3.182 0l5.159 5.159m-1.5-1.5 1.409-1.409a2.25 2.25 0 0 1 3.182 0l2.909 2.909M3.75 21h16.5a2.25 2.25 0 0 0 2.25-2.25V5.25A2.25 2.25 0 0 0 20.25 3H3.75A2.25 2.25 0 0 0 1.5 5.25v13.5A2.25 2.25 0 0 0 3.75 21Z" />
        </svg>
      </div>
    );
  }

  return (
    <div className="relative mb-3 overflow-hidden rounded-lg bg-[var(--color-bg-tertiary)]">
      {media.lqip && !loaded && (
        <img
          src={media.lqip}
          alt=""
          className="absolute inset-0 h-full w-full scale-110 object-cover blur-md"
          draggable={false}
        />
      )}
      <img
        src={thumbUrl}
        alt=""
        className={`w-full object-cover transition-all duration-500 ease-out ${
          loaded ? "scale-100 opacity-100 blur-0" : "scale-95 opacity-0 blur-sm"
        }`}
        style={{ maxHeight: "180px" }}
        draggable={false}
        onLoad={() => setLoaded(true)}
      />
      {media.media_type === "video" && (
        <div className="absolute inset-0 flex items-center justify-center">
          <div className="flex h-10 w-10 items-center justify-center rounded-full bg-black/50">
            <svg className="ml-0.5 h-5 w-5 text-white" fill="currentColor" viewBox="0 0 24 24">
              <path d="M8 5v14l11-7z" />
            </svg>
          </div>
        </div>
      )}
    </div>
  );
}

function DetailPanel({ media, collapsed, onToggleCollapse, onDeleted, initialVariantId }: DetailPanelProps) {
  const [activeTab, setActiveTab] = useState<"details" | "captions" | "tags">("details");
  const [showDeleteConfirm, setShowDeleteConfirm] = useState(false);
  const [showDeleteVariantConfirm, setShowDeleteVariantConfirm] = useState(false);
  const [showClearTagsConfirm, setShowClearTagsConfirm] = useState(false);

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
  const [importVersionPaths, setImportVersionPaths] = useState<string[]>([]);
  const [importingVersion, setImportingVersion] = useState(false);

  // Captions state
  const [captions, setCaptions] = useState<Caption[]>([]);
  const [embeddings, setEmbeddings] = useState<EmbeddingInfo[]>([]);
  const [newCaptionText, setNewCaptionText] = useState("");
  const [editingCaptionId, setEditingCaptionId] = useState<string | null>(null);
  const [editingText, setEditingText] = useState("");
  const [targetId, setTargetId] = useState<string | null>(null); // null=original, string=variant_id
  const [showVersionForm, setShowVersionForm] = useState(false);
  const [showAiEdit, setShowAiEdit] = useState(false);
  const [showExportDialog, setShowExportDialog] = useState(false);
  const [showTargetMenu, setShowTargetMenu] = useState(false);
  const targetMenuRef = useRef<HTMLDivElement>(null);
  const [copiedCaptionId, setCopiedCaptionId] = useState<string | null>(null);

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

  const loadEmbeddings = useCallback(async (mediaId: string, variantId?: string | null) => {
    try {
      const list = await embeddingInfo(mediaId, variantId ?? null);
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
      // When initialVariantId is explicitly provided (browse item click), use it;
      // otherwise fall back to the media's display_variant_id (old behavior).
      const effectiveVariantId = initialVariantId !== undefined ? initialVariantId : (media.display_variant_id ?? null);
      setTargetId(effectiveVariantId);
      setShowVersionForm(false);
      loadMediaTags(media.id, null);
      loadVariants(media.id);
      loadCaptions(media.id);
      loadEmbeddings(media.id, effectiveVariantId);
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
  }, [media?.id, initialVariantId, loadMediaTags, loadVariants, loadCaptions, loadEmbeddings]);


  // Reload tags when target changes
  useEffect(() => {
    if (media) {
      loadMediaTags(media.id, targetId);
    }
  }, [targetId, media?.id, loadMediaTags]);

  // Reload embeddings when target changes (original vs variant)
  useEffect(() => {
    if (media) {
      loadEmbeddings(media.id, targetId);
    }
  }, [targetId, media?.id, loadEmbeddings]);

  // Auto-refresh captions, tags & embeddings when AI annotation completes
  useEffect(() => {
    const unlisten = listen<{ remaining: number }>("ai-task-done", () => {
      if (media) {
        loadCaptions(media.id);
        loadMediaTags(media.id, targetId);
        loadEmbeddings(media.id, targetId);
      }
    });
    return () => { unlisten.then((f) => f()); };
  }, [media?.id, targetId, loadCaptions, loadMediaTags, loadEmbeddings]);

  // Reload tags when batch-tagging or other manual tag changes happen
  useEffect(() => {
    const handler = () => {
      if (media) loadMediaTags(media.id, targetId);
    };
    window.addEventListener("tags-changed", handler);
    return () => window.removeEventListener("tags-changed", handler);
  }, [media?.id, targetId, loadMediaTags]);

  // Reload variants when AI editing adds new variants
  useEffect(() => {
    const handler = (e: Event) => {
      const detail = (e as CustomEvent).detail;
      if (media && detail?.mediaId === media.id) {
        loadVariants(media.id);
      }
    };
    window.addEventListener("variants-changed", handler);
    return () => window.removeEventListener("variants-changed", handler);
  }, [media?.id, loadVariants]);

  // Click outside target menu → close
  useEffect(() => {
    if (!showTargetMenu) return;
    const handler = (e: MouseEvent) => {
      if (targetMenuRef.current && !targetMenuRef.current.contains(e.target as Node)) {
        setShowTargetMenu(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [showTargetMenu]);

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

  const handleClearAllTags = async () => {
    if (!media || tags.length === 0) return;
    try {
      await mediaTagsClear(media.id);
      await loadMediaTags(media.id, targetId);
      showToast(`已清除 ${tags.length} 个标签`);
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
    const paths = importVersionPaths
      .map((p) => p.trim())
      .filter((p) => p.length > 0);
    if (paths.length === 0) return;
    setImportingVersion(true);
    try {
      // Process in chunks of 6 to avoid overwhelming CPU/I/O with parallel
      // image decodes, file copies, and DB writes.
      const CONCURRENCY = 6;
      for (let i = 0; i < paths.length; i += CONCURRENCY) {
        const chunk = paths.slice(i, i + CONCURRENCY);
        await Promise.all(
          chunk.map(async (p) => {
            const v = await variantImport(media.id, p);
            variantAnnotate(media.id, v.id).catch((e) =>
              console.error("Failed to annotate imported version:", e),
            );
          }),
        );
      }
      await loadVariants(media.id);
      setImportVersionPaths([]);
      setShowVersionForm(false);
    } catch (e) {
      console.error("Failed to import version:", e);
      showToast(`导入失败: ${e}`);
    } finally {
      setImportingVersion(false);
    }
  };

  const handleDeleteVariant = async () => {
    if (!targetId || !media) return;
    try {
      await variantDelete(targetId);
      setShowDeleteVariantConfirm(false);
      setTargetId(null);
      await loadVariants(media.id);
      showToast("已删除版本");
    } catch (e) {
      console.error("Failed to delete variant:", e);
    }
  };

  const handleAddCaption = async () => {
    if (!media) return;
    const text = newCaptionText.trim();
    if (!text) return;
    try {
      let caption: Caption;
      if (targetId) {
        caption = await captionCreateForVariant(media.id, targetId, text);
      } else {
        caption = await captionCreate(media.id, text);
      }
      // Insert into local state immediately — no need to reload
      setCaptions((prev) => [caption, ...prev]);
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
      // Update local state immediately — no need to reload
      setCaptions((prev) =>
        prev.map((c) => (c.id === editingCaptionId ? { ...c, text } : c))
      );
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
      // Optimistic: remove from local state immediately
      setCaptions((prev) => prev.filter((c) => c.id !== id));
      await captionDelete(id);
      showToast("已删除描述");
    } catch (e) {
      console.error("Failed to delete caption:", e);
      // Revert on failure
      if (media) loadCaptions(media.id);
    }
  };

  const [adoptingCaptionText, setAdoptingCaptionText] = useState<string | null>(null);

  const handleAdoptAiCaption = async (text: string) => {
    if (!media || adoptingCaptionText) return;
    setAdoptingCaptionText(text);
    // Optimistic: insert a temp entry immediately so UI responds on first click
    const tempId = "temp-" + Math.random().toString(36).slice(2);
    const optimistic: Caption = {
      id: tempId,
      media_id: media.id,
      variant_id: targetId ?? null,
      text,
      source: null,
      created_at: null,
      updated_at: null,
    };
    setCaptions((prev) => [optimistic, ...prev]);
    try {
      let caption: Caption;
      if (targetId) {
        caption = await captionCreateForVariant(media.id, targetId, text);
      } else {
        caption = await captionCreate(media.id, text);
      }
      // Replace temp with real caption
      setCaptions((prev) => prev.map((c) => (c.id === tempId ? caption : c)));
    } catch (e) {
      console.error("Failed to adopt AI caption:", e);
      setCaptions((prev) => prev.filter((c) => c.id !== tempId));
    } finally {
      setAdoptingCaptionText(null);
    }
  };

  const [entered, setEntered] = useState(false);
  useEffect(() => {
    const raf = requestAnimationFrame(() => setEntered(true));
    return () => cancelAnimationFrame(raf);
  }, []);

  if (!media) return null;

  return (
    <>
    {/* Collapsed bar — slides in when collapsed */}
    <div className={`absolute inset-y-0 right-0 z-[41] w-10 transition-transform duration-300 ease-in-out ${collapsed ? "translate-x-0" : "translate-x-full"}`}>
      <div className="flex h-full w-10 flex-col items-center border-l border-[var(--color-border)] bg-[var(--color-bg-secondary)] py-3">
        <button
          onClick={onToggleCollapse}
          className="mb-2 rounded p-0.5 text-[var(--color-text-muted)] hover:bg-[var(--color-bg-hover)] hover:text-[var(--color-text-primary)] transition-colors"
          title="展开详情"
        >
          <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
            <path strokeLinecap="round" strokeLinejoin="round" d="M15.75 19.5 8.25 12l7.5-7.5" />
          </svg>
        </button>
        <p className="text-[10px] text-[var(--color-text-muted)]" style={{ writingMode: "vertical-rl" }}>
          详情
        </p>
      </div>
    </div>

    {/* Expanded panel — slides in when not collapsed */}
    <div className={`absolute inset-y-0 right-0 z-40 w-80 ${entered ? "transition-transform duration-300 ease-in-out" : ""} ${!collapsed && entered ? "translate-x-0" : "translate-x-full"}`}>
    <div className="flex h-full w-80 flex-col border-l border-[var(--color-border)] bg-[var(--color-bg-secondary)] p-4 shadow-2xl shadow-black/20">
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
        <div className="relative flex-1" ref={targetMenuRef}>
          <button
            onClick={() => setShowTargetMenu((s) => !s)}
            className="flex w-full items-center justify-between gap-2 rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none hover:border-[var(--color-text-muted)] transition-colors"
          >
            <span className="truncate">
              {targetId
                ? variants.find((v) => v.id === targetId)?.label || variants.find((v) => v.id === targetId)?.preset_name || "未命名版本"
                : "原图"}
            </span>
            <svg className="h-3 w-3 shrink-0 text-[var(--color-text-muted)]" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
              <path strokeLinecap="round" strokeLinejoin="round" d="m19.5 8.25-7.5 7.5-7.5-7.5" />
            </svg>
          </button>
          <div className={`${showTargetMenu ? "" : "hidden"}`}>
            <TargetMenu
              media={media}
              variants={variants}
              targetId={targetId}
              onSelect={(id) => {
                setTargetId(id);
                setShowTargetMenu(false);
              }}
              onAdd={() => { setShowTargetMenu(false); setShowVersionForm(true); }}
            />
          </div>
        </div>
        {media.display_variant_id && (
          targetId === media.display_variant_id ? (
            <span className="text-xs text-[var(--color-accent)]" title="当前显示版本">👁</span>
          ) : targetId ? null : (
            <span className="text-xs text-[var(--color-accent)]" title="当前显示为其他版本">👁</span>
          )
        )}
      </div>

      <ThumbnailPreview media={media} targetId={targetId} variants={variants} />

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

      {activeTab === "details" && (() => {
          const t = targetId ? variants.find((v) => v.id === targetId) : null;
          // Variant selected but not yet loaded — suppress render to avoid flash of original data
          if (targetId && !t) return null;
          const dimWidth = t?.width ?? media.width;
          const dimHeight = t?.height ?? media.height;
          const fileSize = t?.file_size ?? media.file_size;
          const isVar = !!t;
          return (
        <div className="flex flex-1 flex-col overflow-hidden">
          <div className="flex-1 overflow-auto">
          <div className="space-y-3 text-sm">
            {!isVar && (
            <div>
              <p className="text-xs text-[var(--color-text-muted)]">ID</p>
              <p className="mt-0.5 break-all font-mono text-xs text-[var(--color-text-secondary)]">
                {media.id}
              </p>
            </div>
            )}

            {isVar && t && (
            <div>
              <p className="text-xs text-[var(--color-text-muted)]">版本 ID</p>
              <p className="mt-0.5 break-all font-mono text-[11px] text-[var(--color-text-secondary)]">
                {t.id}
              </p>
            </div>
            )}

            {isVar && (
            <div>
              <p className="text-xs text-[var(--color-text-muted)]">格式</p>
              <p className="mt-0.5 text-[var(--color-text-secondary)]">
                {t?.format.toUpperCase()}
                {t?.quality && t?.format === "jpeg" ? ` · Q${t.quality}` : ""}
              </p>
            </div>
            )}

            <div>
              <p className="text-xs text-[var(--color-text-muted)]">尺寸</p>
              <p className="mt-0.5 text-[var(--color-text-secondary)]">
                {dimWidth ?? "?"} × {dimHeight ?? "?"} px
              </p>
            </div>

            {/* Video metadata */}
            {media.media_type === "video" && (
              <>
                {media.duration != null && (
                  <div className="flex items-center justify-between py-1.5 border-b border-[var(--color-border-light)]">
                    <span className="text-xs text-[var(--color-text-muted)]">时长</span>
                    <span className="text-xs text-[var(--color-text-primary)] tabular-nums">
                      {formatDurationChinese(media.duration)}
                    </span>
                  </div>
                )}
                {media.video_codec != null && (
                  <div className="flex items-center justify-between py-1.5 border-b border-[var(--color-border-light)]">
                    <span className="text-xs text-[var(--color-text-muted)]">编码</span>
                    <span className="text-xs text-[var(--color-text-primary)]">{media.video_codec}</span>
                  </div>
                )}
                {media.video_fps != null && (
                  <div className="flex items-center justify-between py-1.5 border-b border-[var(--color-border-light)]">
                    <span className="text-xs text-[var(--color-text-muted)]">帧率</span>
                    <span className="text-xs text-[var(--color-text-primary)] tabular-nums">
                      {media.video_fps.toFixed(2)} fps
                    </span>
                  </div>
                )}
              </>
            )}

            <div>
              <p className="text-xs text-[var(--color-text-muted)]">文件大小</p>
              <p className="mt-0.5 text-[var(--color-text-secondary)]">
                {formatFileSize(fileSize)}
              </p>
            </div>

            {isVar && t?.source && (
            <div>
              <p className="text-xs text-[var(--color-text-muted)]">来源</p>
              <p className="mt-0.5 text-[var(--color-text-secondary)]">
                {t.source === "generated" && "生成"}
                {t.source === "imported" && "导入"}
                {t.source !== "generated" && t.source !== "imported" && t.source}
              </p>
            </div>
            )}

            {!isVar && media.source !== "web" && (
              <div>
                <p className="text-xs text-[var(--color-text-muted)]">原始路径</p>
                <p className="mt-0.5 break-all text-xs text-[var(--color-text-secondary)]">
                  {media.source_path ?? "—"}
                </p>
              </div>
            )}

            {!isVar && media.source && (
              <div>
                <p className="text-xs text-[var(--color-text-muted)]">来源</p>
                <p className="mt-0.5 text-xs text-[var(--color-text-secondary)]">
                  {media.source === "web" && `网页 · ${parsePlatform(media.page_url || media.source_url) || "未知站点"}`}
                  {media.source === "local" && "本地"}
                  {media.source === "zip" && "ZIP 导入"}
                  {media.source !== "web" && media.source !== "local" && media.source !== "zip" && media.source}
                </p>
              </div>
            )}
            {!isVar && media.source_url && (
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
            {!isVar && media.page_url && (
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

            {!isVar && (
            <div>
              <p className="text-xs text-[var(--color-text-muted)]">导入时间</p>
              <p className="mt-0.5 text-[var(--color-text-secondary)]">
                {formatDate(media.imported_at)}
              </p>
            </div>
            )}
          </div>

            {embeddings.length > 0 && (
          <div className="mt-6 border-t border-[var(--color-border)] pt-4">
            <p className="mb-2 text-xs text-[var(--color-text-muted)]">向量嵌入</p>
            <div className="space-y-1.5">
              {embeddings.map((e) => (
                <div
                  key={e.content_type}
                  className="flex items-center justify-between rounded bg-[var(--color-bg-tertiary)]/50 px-2 py-1.5 group"
                >
                  <span className="text-xs text-[var(--color-text-secondary)]">描述向量</span>
                  <div className="flex items-center gap-2">
                    <span className="text-[10px] text-[var(--color-text-muted)]">
                      {e.vec_dim}d · {e.model}
                    </span>
                    <button
                      onClick={async () => {
                        try {
                          await embeddingDelete(media.id, targetId);
                          setEmbeddings([]);
                        } catch (err) {
                          console.error("Failed to delete embedding:", err);
                        }
                      }}
                      className="opacity-0 group-hover:opacity-100 text-[var(--color-text-muted)] hover:text-[var(--color-danger)] transition-all"
                      title="删除向量"
                    >
                      <svg className="h-3 w-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                        <path strokeLinecap="round" strokeLinejoin="round" d="M6 18 18 6M6 6l12 12" />
                      </svg>
                    </button>
                  </div>
                </div>
              ))}
            </div>
          </div>
            )}
          </div>
        </div>
      );
      })()}

      {activeTab === "tags" && (
        <div className="flex flex-1 flex-col overflow-hidden">
          <div className="flex-1 overflow-auto">
            {(() => {
              if (targetId && !variants.some((v) => v.id === targetId)) {
                return <p className="py-4 text-center text-xs text-[var(--color-text-muted)]">版本未找到</p>;
              }
              return (
                <>
                  {tags.length > 0 && (
                    <div className="mb-2 flex items-center justify-between">
                      <span className="text-[11px] text-[var(--color-text-muted)]">{tags.length} 个标签</span>
                      <button
                        onClick={() => setShowClearTagsConfirm(true)}
                        className="text-[11px] text-[var(--color-text-muted)] hover:text-[var(--color-danger)] transition-colors"
                      >
                        清除全部
                      </button>
                    </div>
                  )}
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
                  const isAi = c.source === "ai" || c.source === "ai_en" || c.source === "ai_zh";
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
                          <div className="mt-2 flex items-center gap-2">
                            <button
                              onClick={async () => {
                                try {
                                  await navigator.clipboard.writeText(c.text);
                                  setCopiedCaptionId(c.id);
                                  setTimeout(() => setCopiedCaptionId(null), 1500);
                                } catch {
                                  // clipboard may not be available
                                }
                              }}
                              className="text-[10px] text-[var(--color-text-muted)] hover:text-[var(--color-accent)] transition-colors"
                              title="复制描述"
                            >
                              {copiedCaptionId === c.id ? (
                                <svg className="h-3 w-3 text-[var(--color-success)]" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                                  <path strokeLinecap="round" strokeLinejoin="round" d="m4.5 12.75 6 6 9-13.5" />
                                </svg>
                              ) : (
                                <svg className="h-3 w-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                                  <path strokeLinecap="round" strokeLinejoin="round" d="M15.666 3.888A2.25 2.25 0 0 0 13.5 2.25h-3c-1.03 0-1.9.693-2.166 1.638m7.332 0c.055.194.084.4.084.612v0a.75.75 0 0 1-.75.75H9a.75.75 0 0 1-.75-.75v0c0-.212.03-.418.084-.612m7.332 0c.646.049 1.288.11 1.927.184 1.1.128 1.907 1.077 1.907 2.185V19.5a2.25 2.25 0 0 1-2.25 2.25H6.75A2.25 2.25 0 0 1 4.5 19.5V6.257c0-1.108.806-2.057 1.907-2.185a48.208 48.208 0 0 1 1.927-.184" />
                                </svg>
                              )}
                            </button>
                            {isAi ? (
                              <button
                                onClick={() => handleAdoptAiCaption(c.text)}
                                disabled={adoptingCaptionText === c.text}
                                className="text-[10px] text-[var(--color-accent)] hover:text-[var(--color-accent-hover)] disabled:opacity-50"
                              >
                                {adoptingCaptionText === c.text ? "采纳中..." : "采纳为手动描述"}
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
              value={importVersionPaths.length === 0 ? "" : importVersionPaths.length === 1 ? importVersionPaths[0] : `已选择 ${importVersionPaths.length} 个文件`}
              onChange={(e) => setImportVersionPaths(e.target.value ? [e.target.value] : [])}
              placeholder="外部文件路径..."
              onKeyDown={(e) => { if (e.key === "Enter") handleImportVersion(); }}
              readOnly={importVersionPaths.length > 1}
              className="flex-1 rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1 text-xs text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)]"
            />
            <button
              onClick={async () => {
                const selected = await open({ multiple: true, filters: [{ name: "图片/视频", extensions: ["jpg", "jpeg", "png", "webp", "gif", "bmp", "mp4", "webm", "mkv", "avi", "mov"] }] });
                if (selected) setImportVersionPaths(Array.isArray(selected) ? selected : [selected]);
              }}
              className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
              title="选择文件（可多选）"
            >...</button>
            <button
              onClick={handleImportVersion}
              disabled={importVersionPaths.length === 0 || importingVersion}
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
            onClick={() => setShowExportDialog(true)}
            className="rounded-lg p-2 text-[var(--color-text-muted)] hover:bg-[var(--color-accent-soft)] hover:text-[var(--color-accent)] transition-colors"
            title="导出"
          >
            <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M3 16.5v2.25A2.25 2.25 0 0 0 5.25 21h13.5A2.25 2.25 0 0 0 21 18.75V16.5M16.5 12 12 16.5m0 0L7.5 12m4.5 4.5V3" />
            </svg>
          </button>
          <button
            onClick={() => setShowAiEdit(true)}
            className="rounded-lg p-2 text-[var(--color-text-muted)] hover:bg-[var(--color-accent-soft)] hover:text-[var(--color-accent)] transition-colors"
            title="AI 编辑"
          >
            <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M9.813 15.904 9 18.75l-.813-2.846a4.5 4.5 0 0 0-3.09-3.09L2.25 12l2.846-.813a4.5 4.5 0 0 0 3.09-3.09L9 5.25l.813 2.846a4.5 4.5 0 0 0 3.09 3.09L15.75 12l-2.846.813a4.5 4.5 0 0 0-3.09 3.09Z" />
            </svg>
          </button>
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
                await loadEmbeddings(media.id, targetId);
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
          {targetId ? (
            <button
              onClick={() => setShowDeleteVariantConfirm(true)}
              className="rounded-lg p-2 text-[var(--color-text-muted)] hover:bg-[var(--color-danger-soft)] hover:text-[var(--color-danger)] transition-colors"
              title="删除版本"
            >
              <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                <path strokeLinecap="round" strokeLinejoin="round" d="m14.74 9-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 0 1-2.244 2.077H8.084a2.25 2.25 0 0 1-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 0 0-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 0 1 3.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 0 0-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 0 0-7.5 0" />
              </svg>
            </button>
          ) : (
            <button
              onClick={() => setShowDeleteConfirm(true)}
              className="rounded-lg p-2 text-[var(--color-text-muted)] hover:bg-[var(--color-danger-soft)] hover:text-[var(--color-danger)] transition-colors"
              title="删除图片"
            >
              <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                <path strokeLinecap="round" strokeLinejoin="round" d="m14.74 9-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 0 1-2.244 2.077H8.084a2.25 2.25 0 0 1-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 0 0-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 0 1 3.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 0 0-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 0 0-7.5 0" />
              </svg>
            </button>
          )}
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

      <ConfirmDialog
        open={showDeleteVariantConfirm}
        title="删除版本"
        message="确定要删除这个版本吗？版本文件和记录将被永久删除，不可恢复。"
        variant="danger"
        confirmLabel="删除"
        onConfirm={handleDeleteVariant}
        onCancel={() => setShowDeleteVariantConfirm(false)}
      />

      <ConfirmDialog
        open={showClearTagsConfirm}
        title="清除所有标签"
        message={`确定要清除这张图片的所有标签吗？将移除 ${tags.length} 个标签，此操作不可撤销。`}
        variant="danger"
        confirmLabel="清除"
        onConfirm={async () => {
          await handleClearAllTags();
          setShowClearTagsConfirm(false);
        }}
        onCancel={() => setShowClearTagsConfirm(false)}
      />
    </div>
    </div>
    {showAiEdit && media && (
      <ImagineDialog
        mediaId={media.id}
        variantId={targetId}
        variantPath={targetId
          ? variants.find((v) => v.id === targetId)?.file_path ?? null
          : media.thumb_256 ?? null}
        onClose={() => setShowAiEdit(false)}
      />
    )}
    {showExportDialog && media && (
      <ExportDialog
        mediaIds={[media.id]}
        variantIds={targetId ? [targetId] : undefined}
        hasOriginals={!targetId}
        totalCount={1}
        onClose={() => setShowExportDialog(false)}
      />
    )}
  </>
  );
}

export default DetailPanel;
