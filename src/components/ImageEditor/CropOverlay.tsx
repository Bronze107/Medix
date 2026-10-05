import { useRef } from "react";
import {
  CORNER_HANDLES,
  EDGE_HANDLES,
  applyDrag,
  clampFreeRect,
  displayFromOriginal,
  fitAspectRect,
  type HandleKind,
  type Rect,
  type Size,
} from "@/lib/editor/geometry";

interface CropOverlayProps {
  /** 原图像素尺寸（应与 Rust 按 EXIF 方向解码后的尺寸一致） */
  origSize: Size;
  /** 覆盖层的 CSS 尺寸，即图像在屏幕上的显示框 */
  displaySize: Size;
  /** 当前裁剪矩形，原图空间 */
  rect: Rect;
  /** 锁定的宽高比（w/h）；null 为自由裁剪 */
  aspect: number | null;
  onChange: (rect: Rect) => void;
}

const HANDLE_CURSOR: Record<string, string> = {
  nw: "nwse-resize",
  n: "ns-resize",
  ne: "nesw-resize",
  e: "ew-resize",
  se: "nwse-resize",
  s: "ns-resize",
  sw: "nesw-resize",
  w: "ew-resize",
};

/** 手柄在选框内的相对位置：0 = 左/上边，1 = 右/下边，0.5 = 居中。 */
function handleFraction(kind: HandleKind): { fx: number; fy: number } {
  return {
    fx: kind.includes("w") ? 0 : kind.includes("e") ? 1 : 0.5,
    fy: kind.includes("n") ? 0 : kind.includes("s") ? 1 : 0.5,
  };
}

/**
 * 裁剪选框：遮罩 + 边框 + 三分线 + 手柄。
 *
 * 指针位移换算**只需要 displaySize**（不需要 getBoundingClientRect），所以这块
 * 能在 jsdom 里直接测：给一组 clientX/clientY 即可断言产出的矩形。
 */
export function CropOverlay({
  origSize,
  displaySize,
  rect,
  aspect,
  onChange,
}: CropOverlayProps) {
  const dragRef = useRef<{
    kind: HandleKind;
    clientX: number;
    clientY: number;
    start: Rect;
  } | null>(null);

  const startDrag = (e: React.PointerEvent, kind: HandleKind) => {
    e.preventDefault();
    e.stopPropagation();
    // jsdom 未实现 setPointerCapture，可选调用
    e.currentTarget.setPointerCapture?.(e.pointerId);
    dragRef.current = { kind, clientX: e.clientX, clientY: e.clientY, start: rect };
  };

  const onPointerMove = (e: React.PointerEvent) => {
    const drag = dragRef.current;
    if (!drag || !(displaySize.w > 0) || !(displaySize.h > 0)) return;
    // 显示像素 → 原图像素
    const dx = ((e.clientX - drag.clientX) * origSize.w) / displaySize.w;
    const dy = ((e.clientY - drag.clientY) * origSize.h) / displaySize.h;
    const next = applyDrag(drag.start, drag.kind, dx, dy, aspect);
    // 非法结果一律丢弃：保证 state 里不会出现退化或越界矩形
    const clamped = aspect ? fitAspectRect(next, origSize, aspect) : clampFreeRect(next, origSize);
    if (clamped) onChange(clamped);
  };

  const endDrag = (e: React.PointerEvent) => {
    e.currentTarget.releasePointerCapture?.(e.pointerId);
    dragRef.current = null;
  };

  const tl = displayFromOriginal(rect.x, rect.y, origSize, displaySize);
  const br = displayFromOriginal(rect.x + rect.w, rect.y + rect.h, origSize, displaySize);
  const box = { left: tl.x, top: tl.y, width: br.x - tl.x, height: br.y - tl.y };

  // 锁比例时只给角手柄：边手柄在锁定比例下语义含糊（见 geometry.applyDrag 注释）
  const handles: HandleKind[] = aspect
    ? CORNER_HANDLES
    : [...CORNER_HANDLES, ...EDGE_HANDLES];

  const shade = "absolute bg-[var(--color-bg-overlay)]";

  return (
    <div
      className="absolute inset-0 touch-none"
      data-testid="crop-overlay"
      onPointerMove={onPointerMove}
      onPointerUp={endDrag}
      onPointerCancel={endDrag}
    >
      {/* 选框外的四块遮罩（不用 box-shadow：四块的位置可以直接断言） */}
      <div className={shade} style={{ left: 0, top: 0, right: 0, height: Math.max(box.top, 0) }} />
      <div className={shade} style={{ left: 0, top: box.top + box.height, right: 0, bottom: 0 }} />
      <div
        className={shade}
        style={{ left: 0, top: box.top, width: Math.max(box.left, 0), height: box.height }}
      />
      <div
        className={shade}
        style={{ left: box.left + box.width, top: box.top, right: 0, height: box.height }}
      />

      {/* 选框本体：拖它 = 平移 */}
      <div
        className="absolute cursor-move border border-white/90"
        style={box}
        onPointerDown={(e) => startDrag(e, "move")}
      >
        <div className="pointer-events-none absolute inset-0">
          <div className="absolute top-0 bottom-0 border-l border-white/25" style={{ left: "33.333%" }} />
          <div className="absolute top-0 bottom-0 border-l border-white/25" style={{ left: "66.666%" }} />
          <div className="absolute left-0 right-0 border-t border-white/25" style={{ top: "33.333%" }} />
          <div className="absolute left-0 right-0 border-t border-white/25" style={{ top: "66.666%" }} />
        </div>
      </div>

      {handles.map((kind) => {
        const { fx, fy } = handleFraction(kind);
        return (
          <div
            key={kind}
            role="button"
            tabIndex={-1}
            aria-label={`裁剪手柄 ${kind}`}
            data-handle={kind}
            className="absolute h-3 w-3 -translate-x-1/2 -translate-y-1/2 rounded-sm border border-white bg-white/30"
            style={{
              left: box.left + box.width * fx,
              top: box.top + box.height * fy,
              cursor: HANDLE_CURSOR[kind],
            }}
            onPointerDown={(e) => startDrag(e, kind)}
          />
        );
      })}
    </div>
  );
}
