/**
 * 笔迹的数据模型与采样。
 *
 * **笔迹坐标一律存原图像素空间**，不存屏幕/工作空间坐标。这样裁剪与涂抹的先后
 * 顺序都不影响结果：改裁剪只是重算映射并把超出部分裁掉，不需要重映射已有笔迹。
 */

export interface Point {
  x: number;
  y: number;
}

export interface Stroke {
  id: string;
  /** 原图空间坐标 */
  points: Point[];
  /** 笔尖直径（原图像素） */
  size: number;
  /** 边缘羽化：1 = 实心圆，0 = 完全渐隐 */
  hardness: number;
  /** 0..1 */
  opacity: number;
  /** CSS 颜色；橡皮擦忽略该值 */
  color: string;
  /** 橡皮擦：以 destination-out 擦掉已画内容 */
  erase: boolean;
}

/** 相邻笔尖的最大间距（相对笔尖直径），越小越平滑、越慢。 */
export const STAMP_SPACING_RATIO = 0.25;

/**
 * 沿折线按固定间距插值出笔尖落点（含第一个点）。
 *
 * 为什么插值而不是用 `lineTo`：硬度 < 1 需要径向渐变笔尖，而渐变只能按「点」
 * 绘制；把落点均匀铺在路径上就能做出连续的软边笔画。间距取笔尖直径的
 * `STAMP_SPACING_RATIO`，低于此值会看出一个个独立的圆。
 */
export function stampCenters(points: Point[], size: number): Point[] {
  if (points.length === 0) return [];
  // 单点（点一下）或退化间距：原样返回，避免死循环
  const spacing = size * STAMP_SPACING_RATIO;
  if (points.length === 1 || !(spacing > 0)) return [...points];

  const out: Point[] = [points[0]];
  // 从上一段落点算起的已走距离，保证跨段时间距连续
  let carry = 0;

  for (let i = 1; i < points.length; i++) {
    const a = points[i - 1];
    const b = points[i];
    const dx = b.x - a.x;
    const dy = b.y - a.y;
    const len = Math.hypot(dx, dy);
    if (len === 0) continue;

    for (let t = spacing - carry; t <= len; t += spacing) {
      const k = t / len;
      out.push({ x: a.x + dx * k, y: a.y + dy * k });
    }
    carry = (carry + len) % spacing;
  }

  return out;
}

/** 笔尖直径一半（径向渐变的半径）。 */
export function tipRadius(size: number): number {
  return Math.max(size / 2, 0.5);
}

/**
 * 某条笔迹在给定变换下是否需要绘制——点全在裁剪区外就跳过。
 * 只是省开销的粗判，正确的裁剪仍由 canvas `clip()` 保证。
 */
export function strokeIntersectsRect(
  stroke: Stroke,
  rect: { x: number; y: number; w: number; h: number },
): boolean {
  const r = tipRadius(stroke.size);
  return stroke.points.some(
    (p) =>
      p.x + r >= rect.x &&
      p.x - r <= rect.x + rect.w &&
      p.y + r >= rect.y &&
      p.y - r <= rect.y + rect.h,
  );
}
