import { useCallback } from "react";

interface DropZoneProps {
  onDropFiles: (files: File[]) => void;
}

function DropZone({ onDropFiles }: DropZoneProps) {
  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const files = Array.from(e.dataTransfer.files);
      if (files.length > 0) {
        onDropFiles(files);
      }
    },
    [onDropFiles]
  );

  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
  }, []);

  return (
    <div
      onDrop={handleDrop}
      onDragOver={handleDragOver}
      className="flex flex-1 flex-col items-center justify-center rounded-xl border-2 border-dashed border-neutral-700 bg-neutral-800/30 p-8 transition-colors hover:border-neutral-500 hover:bg-neutral-800/50"
    >
      <svg
        className="mb-4 h-12 w-12 text-neutral-500"
        fill="none"
        viewBox="0 0 24 24"
        stroke="currentColor"
        strokeWidth={1}
      >
        <path
          strokeLinecap="round"
          strokeLinejoin="round"
          d="m2.25 15.75 5.159-5.159a2.25 2.25 0 0 1 3.182 0l5.159 5.159m-1.5-1.5 1.409-1.409a2.25 2.25 0 0 1 3.182 0l2.909 2.909M3.75 21h16.5a2.25 2.25 0 0 0 2.25-2.25V5.25A2.25 2.25 0 0 0 20.25 3H3.75A2.25 2.25 0 0 0 1.5 5.25v13.5A2.25 2.25 0 0 0 3.75 21Z"
        />
      </svg>
      <p className="text-sm text-neutral-400">拖入图片到这里导入</p>
      <p className="mt-1 text-xs text-neutral-600">
        支持 JPG, PNG, WebP, GIF, BMP
      </p>
    </div>
  );
}

export default DropZone;
