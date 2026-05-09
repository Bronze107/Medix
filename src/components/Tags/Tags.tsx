import { useCallback, useEffect, useState } from "react";
import type { Tag } from "@/types/tag";
import { tagList, tagCreate, tagDelete, tagRename } from "@/lib/tauri";

function Tags() {
  const [tags, setTags] = useState<Tag[]>([]);
  const [newTagName, setNewTagName] = useState("");
  const [loading, setLoading] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editName, setEditName] = useState("");

  const loadTags = useCallback(async () => {
    try {
      const list = await tagList();
      setTags(list);
    } catch (e) {
      console.error("Failed to load tags:", e);
    }
  }, []);

  useEffect(() => {
    loadTags();
  }, [loadTags]);

  const handleCreate = async () => {
    const name = newTagName.trim();
    if (!name) return;
    setLoading(true);
    try {
      await tagCreate(name);
      setNewTagName("");
      await loadTags();
    } catch (e) {
      console.error("Failed to create tag:", e);
    } finally {
      setLoading(false);
    }
  };

  const handleDelete = async (id: string, name: string) => {
    if (!window.confirm(`确定要删除标签 "${name}" 吗？关联的图片将不再显示此标签。`)) {
      return;
    }
    try {
      await tagDelete(id);
      await loadTags();
    } catch (e) {
      console.error("Failed to delete tag:", e);
    }
  };

  const startEdit = (tag: Tag) => {
    setEditingId(tag.id);
    setEditName(tag.name);
  };

  const cancelEdit = () => {
    setEditingId(null);
    setEditName("");
  };

  const handleRename = async (id: string) => {
    const name = editName.trim();
    if (!name) {
      cancelEdit();
      return;
    }
    try {
      await tagRename(id, name);
      setEditingId(null);
      await loadTags();
    } catch (e) {
      console.error("Failed to rename tag:", e);
    }
  };

  return (
    <div className="flex h-full flex-col p-6">
      <h1 className="mb-6 text-2xl font-bold">标签管理</h1>

      {/* Create tag */}
      <div className="mb-6 flex items-center gap-3">
        <input
          type="text"
          value={newTagName}
          onChange={(e) => setNewTagName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") handleCreate();
          }}
          placeholder="新建标签..."
          className="flex-1 rounded border border-neutral-700 bg-neutral-800 px-3 py-2 text-sm text-neutral-200 outline-none placeholder:text-neutral-500 focus:border-blue-500"
        />
        <button
          onClick={handleCreate}
          disabled={loading || !newTagName.trim()}
          className="rounded bg-blue-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-blue-500 disabled:opacity-50 disabled:hover:bg-blue-600"
        >
          创建
        </button>
      </div>

      {/* Tag list */}
      <div className="flex-1 overflow-auto">
        {tags.length === 0 ? (
          <p className="text-sm text-neutral-500">暂无标签</p>
        ) : (
          <div className="space-y-2">
            {tags.map((tag) => (
              <div
                key={tag.id}
                className="flex items-center justify-between rounded-lg border border-neutral-800 bg-neutral-800/50 px-4 py-3"
              >
                {editingId === tag.id ? (
                  <div className="flex flex-1 items-center gap-2">
                    <input
                      type="text"
                      value={editName}
                      onChange={(e) => setEditName(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") handleRename(tag.id);
                        if (e.key === "Escape") cancelEdit();
                      }}
                      autoFocus
                      className="flex-1 rounded border border-neutral-600 bg-neutral-900 px-2 py-1 text-sm text-neutral-200 outline-none focus:border-blue-500"
                    />
                    <button
                      onClick={() => handleRename(tag.id)}
                      className="rounded bg-blue-600 px-2 py-1 text-xs text-white hover:bg-blue-500"
                    >
                      保存
                    </button>
                    <button
                      onClick={cancelEdit}
                      className="rounded border border-neutral-600 px-2 py-1 text-xs text-neutral-300 hover:bg-neutral-700"
                    >
                      取消
                    </button>
                  </div>
                ) : (
                  <>
                    <button
                      onClick={() => startEdit(tag)}
                      className="text-sm font-medium text-neutral-300 hover:text-neutral-100"
                    >
                      {tag.name}
                    </button>
                    <button
                      onClick={() => handleDelete(tag.id, tag.name)}
                      className="rounded p-1 text-neutral-500 transition-colors hover:bg-red-900/30 hover:text-red-400"
                      title="删除"
                    >
                      <svg
                        className="h-4 w-4"
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
                  </>
                )}
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

export default Tags;
