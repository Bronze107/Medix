/**
 * 图像编辑器的纯几何计算。
 *
 * 全部为纯函数、不碰 DOM 与 canvas —— 因为 jsdom 里 `getContext("2d")` 返回 null，
 * 画布本身无法单测，而裁剪差一像素、比例漂移、坐标错位这类**真正会出错的地方
 * 都发生在这里**，所以刻意把这块从组件里剥出来。
 *
 * 三个坐标空间：
 * - **原图空间**：原始像素。裁剪矩形只在这个空间里表达，供 Rust 无损 crop_imm。
 * - **工作空间**：原图等比缩到最长边 <= WORK_MAX_DIM。画布底图的固有分辨率。
 * - **显示空间**：工作空间再按容器适配后的 CSS 像素，即用户实际看到的尺寸。
 */

export interface Size {
  w: number;
  h: number;
}

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** 工作画布的最长边上限。12MP 原图直接用 canvas 处理内存与性能都会失控。 */
export const WORK_MAX_DIM = 2048;

/** 裁剪矩形的最小边长（原图像素）。太小的框既不好操作也没有意义。 */
export const MIN_CROP_PX = 8;

/** 矩形在 `x` 方向是否退化到无法使用。 */
function degenerate(rect: Rect): boolean {
  return !(rect.w > 0) || !(rect.h > 0);
}

/** 原图 → 工作分辨率（只缩不放）。 */
export function computeWorkingSize(origW: number, origH: number): Size {
  const long = Math.max(origW, origH);
  if (!(long > 0)) return { w: 1, h: 1 };
  const scale = Math.min(1, WORK_MAX_DIM / long);
  return {
    w: Math.max(1, Math.round(origW * scale)),
    h: Math.max(1, Math.round(origH * scale)),
  };
}

/**
 * 指针客户端坐标 → 工作空间坐标。
 *
 * `rect` 必须传**实时的** `getBoundingClientRect()`，不要用 state 里缓存的显示尺寸：
 * 窗口缩放或亚像素布局会让两者差一点点，而这点误差在画笔落点上看得见。
 */
export function workingFromClient(
  clientX: number,
  clientY: number,
  rect: { left: number; top: number; width: number; height: number },
  work: Size,
): { x: number; y: number } {
  if (!(rect.width > 0) || !(rect.height > 0)) return { x: 0, y: 0 };
  return {
    x: ((clientX - rect.left) * work.w) / rect.width,
    y: ((clientY - rect.top) * work.h) / rect.height,
  };
}

/** 工作空间 → 原图空间。 */
export function originalFromWorking(
  x: number,
  y: number,
  work: Size,
  orig: Size,
): { x: number; y: number } {
  if (!(work.w > 0) || !(work.h > 0)) return { x: 0, y: 0 };
  return { x: (x * orig.w) / work.w, y: (y * orig.h) / work.h };
}

/** 原图空间 → 显示空间（裁剪覆盖层用）。 */
export function displayFromOriginal(
  x: number,
  y: number,
  orig: Size,
  display: Size,
): { x: number; y: number } {
  if (!(orig.w > 0) || !(orig.h > 0)) return { x: 0, y: 0 };
  return { x: (x / orig.w) * display.w, y: (y / orig.h) * display.h };
}

/**
 * 自由裁剪：两轴独立夹取到图像边界内。无比例约束，所以各轴互不影响。
 * 夹取后小于最小尺寸则返回 null。
 */
export function clampFreeRect(rect: Rect, orig: Size, min = MIN_CROP_PX): Rect | null {
  const w = Math.min(rect.w, orig.w);
  const h = Math.min(rect.h, orig.h);
  if (w < min || h < min) return null;
  return {
    x: Math.min(Math.max(rect.x, 0), orig.w - w),
    y: Math.min(Math.max(rect.y, 0), orig.h - h),
    w,
    h,
  };
}

/**
 * 等比裁剪：先整体缩放到能放进图像，再平移进位。
 *
 * **必须整体缩放而不是分轴夹取** —— 分轴夹取会把比例压歪，用户锁了 1:1 却得到
 * 一个 1:1.3 的框。缩放后 w<=orig.w 且 h<=orig.h，所以 `orig.w - w` 必非负。
 */
export function fitAspectRect(
  rect: Rect,
  orig: Size,
  aspect: number,
  min = MIN_CROP_PX,
): Rect | null {
  if (!(aspect > 0) || degenerate(rect)) return null;
  const scale = Math.min(1, orig.w / rect.w, orig.h / rect.h);
  const w = rect.w * scale;
  const h = rect.h * scale;
  if (w < min || h < min) return null;
  return {
    x: Math.min(Math.max(rect.x, 0), orig.w - w),
    y: Math.min(Math.max(rect.y, 0), orig.h - h),
    w,
    h,
  };
}

/**
 * 以 `anchor`（拖拽时不动的那一角）为固定点，按 `aspect` 推出等比矩形。
 *
 * 取「自由拖动框里更宽松的那一轴」定尺寸，跟随手感自然；方向由落点在锚点的
 * 哪一侧决定，所以四个方向拖拽共用同一段逻辑。
 */
export function lockAspectFromAnchor(
  anchor: { x: number; y: number },
  moving: { x: number; y: number },
  aspect: number,
): Rect {
  const dx = moving.x - anchor.x;
  const dy = moving.y - anchor.y;
  const sx = dx < 0 ? -1 : 1;
  const sy = dy < 0 ? -1 : 1;
  const freeW = Math.abs(dx);
  const freeH = Math.abs(dy);

  // freeW/freeH > aspect 说明框比目标比例更宽 → 由高度定尺寸
  let w: number;
  let h: number;
  if (freeH > 0 && freeW / freeH > aspect) {
    h = freeH;
    w = freeH * aspect;
  } else {
    w = freeW;
    h = aspect > 0 ? freeW / aspect : freeH;
  }

  return {
    x: sx > 0 ? anchor.x : anchor.x - w,
    y: sy > 0 ? anchor.y : anchor.y - h,
    w,
    h,
  };
}

/**
 * 序列化成原图像素空间的整数矩形，供 Rust `crop_imm` 使用。
 *
 * 取整只在这里做一次：起点 floor、终点 ceil，宁可多含一行像素也不要少含
 * （少含会把用户框进来的内容切掉）。夹取后小于最小尺寸返回 null。
 */
export function serializeCropRect(rect: Rect, orig: Size, min = MIN_CROP_PX): Rect | null {
  if (degenerate(rect)) return null;
  const x0 = Math.min(Math.max(Math.floor(rect.x), 0), Math.max(orig.w - 1, 0));
  const y0 = Math.min(Math.max(Math.floor(rect.y), 0), Math.max(orig.h - 1, 0));
  const x1 = Math.min(Math.max(Math.ceil(rect.x + rect.w), x0 + 1), orig.w);
  const y1 = Math.min(Math.max(Math.ceil(rect.y + rect.h), y0 + 1), orig.h);
  const w = x1 - x0;
  const h = y1 - y0;
  if (w < min || h < min) return null;
  return { x: x0, y: y0, w, h };
}

/** 整图矩形（初始选框）。 */
export function fullRect(orig: Size): Rect {
  return { x: 0, y: 0, w: orig.w, h: orig.h };
}

/** 拖拽作用点：四角、四边或整体平移。 */
export type HandleKind = "move" | "n" | "s" | "e" | "w" | "ne" | "nw" | "se" | "sw";

export const CORNER_HANDLES: HandleKind[] = ["nw", "ne", "sw", "se"];
export const EDGE_HANDLES: HandleKind[] = ["n", "s", "e", "w"];

/** 角手柄对应的对角锚点（拖拽时保持不动的那个角）。 */
function oppositeCorner(rect: Rect, kind: HandleKind): { x: number; y: number } | null {
  const { x, y, w, h } = rect;
  switch (kind) {
    case "se":
      return { x, y };
    case "sw":
      return { x: x + w, y };
    case "ne":
      return { x, y: y + h };
    case "nw":
      return { x: x + w, y: y + h };
    default:
      return null;
  }
}

/**
 * 拖拽手柄后的新矩形（原图空间，**未夹取**，调用方仍需 clamp/fit）。
 *
 * 锁比例时只从**角手柄**缩放：边手柄在锁定比例下语义含糊（该固定哪条对边？
 * 另一轴往哪边扩？），所以 UI 在锁定比例时只显示角手柄，这里也只需处理四角 ——
 * 传入边手柄会退化成自由缩放。
 *
 * 自由缩放允许拖过对边（翻转），此时归一化成正的宽高而不是产出负尺寸。
 */
export function applyDrag(
  rect: Rect,
  kind: HandleKind,
  dx: number,
  dy: number,
  aspect: number | null,
): Rect {
  if (kind === "move") {
    return { x: rect.x + dx, y: rect.y + dy, w: rect.w, h: rect.h };
  }

  let left = rect.x;
  let top = rect.y;
  let right = rect.x + rect.w;
  let bottom = rect.y + rect.h;

  const touchesLeft = kind === "w" || kind === "nw" || kind === "sw";
  const touchesRight = kind === "e" || kind === "ne" || kind === "se";
  const touchesTop = kind === "n" || kind === "ne" || kind === "nw";
  const touchesBottom = kind === "s" || kind === "se" || kind === "sw";

  if (touchesLeft) left += dx;
  if (touchesRight) right += dx;
  if (touchesTop) top += dy;
  if (touchesBottom) bottom += dy;

  const anchor = aspect && aspect > 0 ? oppositeCorner(rect, kind) : null;
  if (anchor) {
    // 被拖动的那一角：e 侧用 right、w 侧用 left（s/n 同理），不能一律取 right/bottom
    const movingX = touchesRight ? right : left;
    const movingY = touchesBottom ? bottom : top;
    return lockAspectFromAnchor(anchor, { x: movingX, y: movingY }, aspect as number);
  }

  return {
    x: Math.min(left, right),
    y: Math.min(top, bottom),
    w: Math.abs(right - left),
    h: Math.abs(bottom - top),
  };
}
