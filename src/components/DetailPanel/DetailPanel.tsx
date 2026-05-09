import { useCallback, useEffect, useRef, useState } from "react";
import type { Media } from "@/types/media";
import type { Tag } from "@/types/tag";
import {
  mediaTagsGet,
  mediaTagAdd,
  mediaTagRemove,
  tagList,
  tagCreate,
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
  const [tags, setTags] = useState<Tag[]>([]);
  const [allTags, setAllTags] = useState<Tag[]>([]);
  const [newTagInput, setNewTagInput] = useState("");
  const [suggestions, setSuggestions] = useState<Tag[]>([]);
  const [showSuggestions, setShowSuggestions] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

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

  useEffect(() => {
    loadAllTags();
  }, [loadAllTags]);

  useEffect(() => {
    if (media) {
      loadMediaTags(media.id);
    } else {
      setTags([]);
    }
  }, [media?.id, loadMediaTags]);

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

  if (!media) {
    return (
      <div className="flex h-full w-72 flex-col border-l border-neutral-800 bg-neutral-900 p-4">
        <p className="text-sm text-neutral-500">选择一张图片查看详情</p>
      </div>
    );
  }

  return (
    <div className="flex h-full w-72 flex-col border-l border-neutral-800 bg-neutral-900 p-4">
      <h2 className="mb-4 text-sm font-bold text-neutral-200">详情</h2>

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
    </div>
  );
}

export default DetailPanel;
