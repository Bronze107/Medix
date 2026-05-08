import type { Media } from "@/types/media";

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
    </div>
  );
}

export default DetailPanel;
