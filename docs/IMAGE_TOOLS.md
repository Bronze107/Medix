# 图像编辑工具设计文档

> 本文档描述 Medix 内建图像编辑工具的设计方案。目标：在 Lightbox 或详情面板中提供裁剪、画笔、文本叠加等基础编辑能力，结果作为新变体保存，不覆盖原图。

---

## 1. 范围与架构

### 核心原则

- **前端渲染，Rust 保存**：Canvas 负责实时预览和交互，确认后用 Rust `image` crate 或 Canvas blob 完成最终处理
- **不覆盖原图**：编辑结果存为新变体（`source="edited"`），和 AI 编辑一致
- **复用变体系统**：自动生成缩略图、可 AI 标注、可导出

### 架构图

```
用户操作 → Canvas 实时预览
         → 确认 → Canvas.toBlob (画笔/文本) 或 Rust image::crop (裁剪)
                → 保存为变体
                → 自动生成 256px 缩略图
                → 出现在详情面板版本选择器
```

---

## 2. 裁剪 (Crop)

### 交互

```
1. 进入裁剪模式 → 遮罩覆盖原图
2. 用户拖拽选取框 (aspect ratio 可选锁定)
3. 可拖拽四角/四边调整
4. 确认 → 发送 {x, y, width, height} 到 Rust
5. Rust image crate 裁剪 → 保存变体

┌──────────────────────┐
│ ░░░░░░░░░░░░░░░░░░░░ │  遮罩层
│ ░░░░┌────────┐░░░░░ │
│ ░░░░│        │░░░░░ │  选取框
│ ░░░░│  9:16   │░░░░░ │
│ ░░░░└────────┘░░░░░ │
│ ░░░░░░░░░░░░░░░░░░░░ │
└──────────────────────┘
```

### 实现

```rust
// Rust 端
fn crop_variant(app: &AppHandle, media_id: &str, x: u32, y: u32, w: u32, h: u32) -> Result<Variant> {
    let img = image::open(&source_path)?;
    let cropped = img.crop_imm(x, y, w, h);
    // 保存为变体，和 variant_generate 一样
}
```

### 锁定宽高比

工具栏提供常用比例按钮：自由 / 1:1 / 4:3 / 16:9 / 原图比例。锁定时拖拽角按比例缩放。

---

## 3. 画笔 (Brush)

### 交互

```
1. 进入画笔模式 → Canvas 透明层叠在原图上
2. pointerdown → 开始画线
3. pointermove → 实时渲染笔触
4. pointerup → 保存快照用于撤销
5. 确认 → Canvas.toBlob → Rust 保存变体
```

### 笔触

- 圆形笔尖，硬度控制边缘羽化程度
- `ctx.lineCap = "round"`, `ctx.lineJoin = "round"`
- 画笔大小 → `ctx.lineWidth`
- 硬度 100% → 实心圆；硬度 0% → 径向渐变软边

```ts
function drawBrushTip(ctx: CanvasRenderingContext2D, x: number, y: number, size: number, hardness: number, color: string) {
  const gradient = ctx.createRadialGradient(x, y, 0, x, y, size / 2);
  gradient.addColorStop(hardness, color);
  gradient.addColorStop(1, "transparent");
  ctx.fillStyle = gradient;
  ctx.beginPath();
  ctx.arc(x, y, size / 2, 0, Math.PI * 2);
  ctx.fill();
}
```

### 工具栏

| 控件 | 范围 | 默认值 |
|------|------|--------|
| 画笔大小 | 1-100px | 20px |
| 颜色 | color picker | #000000 |
| 硬度 | 0-100% | 80% |
| 不透明度 | 0-100% | 100% |

### 橡皮擦

Canvas `globalCompositeOperation = "destination-out"` 擦除已画内容。橡皮擦大小独立可调。

---

## 4. 文本叠加 (Text)

### 交互

```
1. 进入文本模式 → 点击图片上的位置
2. 弹出文本输入（内联或浮层）
3. 输入文字 → Canvas 实时预览
4. 可拖拽移动文本位置
5. 确认 → Canvas.toBlob → Rust 保存变体
```

### 独立文本层

每个文本块是独立对象：

```ts
interface TextLayer {
  id: string;
  text: string;
  x: number;
  y: number;
  font: string;
  fontSize: number;
  color: string;
  strokeColor: string | null;  // 描边色，null 表示无描边
  strokeWidth: number;
}
```

- 点击已有文本层 → 重新编辑
- 拖拽文本层 → 移动位置
- 双击 → 删除确认
- 支持多个文本层

### 工具栏

| 控件 | 范围 | 默认值 |
|------|------|--------|
| 字体 | 系统字体列表 | Arial |
| 字号 | 8-200px | 48px |
| 颜色 | color picker | #ffffff |
| 描边色 | color picker | #000000 |
| 描边宽 | 0-10px | 2px |

### 渲染

```ts
ctx.font = `${fontSize}px ${font}`;
ctx.fillStyle = color;
if (strokeColor && strokeWidth > 0) {
  ctx.strokeStyle = strokeColor;
  ctx.lineWidth = strokeWidth;
  ctx.strokeText(text, x, y);
}
ctx.fillText(text, x, y);
```

---

## 5. 撤销/重做

### Canvas 快照栈

```ts
const undoStack = useRef<ImageData[]>([]);
const redoStack = useRef<ImageData[]>([]);

function pushSnapshot(ctx: CanvasRenderingContext2D, canvas: HTMLCanvasElement) {
  const snapshot = ctx.getImageData(0, 0, canvas.width, canvas.height);
  undoStack.current.push(snapshot);
  redoStack.current = []; // 新操作清空 redo
  if (undoStack.current.length > 20) undoStack.current.shift(); // 最多 20 步
}

function undo(ctx: CanvasRenderingContext2D) {
  const prev = undoStack.current.pop();
  if (prev) {
    redoStack.current.push(ctx.getImageData(0, 0, canvas.width, canvas.height));
    ctx.putImageData(prev, 0, 0);
  }
}
```

- 每次 `pointerup`（画笔完成一笔）或文本确认时 push 快照
- 全分辨率快照内存开销大（12MP 图片 = 36MB ImageData），限制 20 步
- 小分辨率的代理 Canvas（如 2048px 内）可降低内存

---

## 6. UI 集成

### 入口

| 入口 | 位置 | 行为 |
|------|------|------|
| 详情面板操作栏 | AI 编辑按钮旁边 | 打开编辑模式 |
| Lightbox 工具栏 | 底部工具栏 | 在当前查看的图片/变体上编辑 |
| 右键菜单 | "编辑图片" | 打开 Lightbox 并进入编辑模式 |

### 工具栏布局

```
┌─ Lightbox 底部工具栏 ──────────────────────┐
│                                              │
│  ← →  [✂裁剪] [✏画笔] [T文本]              │
│                                              │
│  ── 画笔工具 ──                              │
│  大小: [═══20═══]  ○颜色 硬度: [══80%══]     │
│                                              │
│   ↩撤销  ↪重做             [取消] [✅确认]    │
└──────────────────────────────────────────────┘
```

- 工具切换时显示对应子工具栏
- 裁剪模式：遮罩 + 比例锁定按钮
- 画笔模式：笔刷参数
- 文本模式：排版参数

### 编辑模式进入/退出

- 进入：Lightbox 全屏 + 底部工具栏 + Canvas 覆盖
- 确认：合成 → 保存变体 → Toast "已保存为新版本" → 刷新详情面板
- 取消：丢弃编辑 → 恢复原图显示 → 关闭工具栏

---

## 7. 结果处理

### 裁剪

```
前端提供 rect{x,y,w,h} → Rust image::crop_imm → 保存变体
```

裁剪本身无损（`image` crate 直接切像素），之后按原格式保存。

### 画笔/文本

```
Canvas.toBlob("image/png") → Rust 接收 PNG 字节 → 保存变体
```

- Canvas 输出 PNG（无损），避免 JPEG 二次压缩
- 变体格式记录为 PNG，与原图格式无关
- 缩略图自动生成（从 PNG）

---

## 8. 不影响现有功能

- 变体系统完全复用：编辑结果和 AI 编辑 / 手动生成变体无差异
- 缩略图自动生成：变体保存时触发
- 标签/Caption：可对编辑变体单独标注
- 导出：变体一起导出
- 版本选择器：新变体自动出现，label 为"编辑" + 工具名（如"画笔编辑"）

---

## 9. 实现路线

### 阶段 A：裁剪（最简）

1. Lightbox 加"编辑"按钮 → 裁剪模式
2. 选取框拖拽 + 比例锁定
3. Rust `crop_variant` 命令
4. 保存变体 + 刷新

### 阶段 B：画笔

1. Canvas 透明层 + pointer 事件
2. 笔触渲染 + 颜色/大小/硬度
3. 橡皮擦
4. 撤销栈
5. Canvas.toBlob → Rust

### 阶段 C：文本

1. 文本层管理（增删改移）
2. 字体/颜色/描边控制
3. Canvas 文本渲染

### 阶段 D：完善

1. 混合工具（同一编辑会话中切换裁剪/画笔/文本）
2. 滤镜（亮度/对比度/饱和度）— 可选
3. 马赛克/模糊 — 可选

---

## 10. 技术风险

| 风险 | 缓解 |
|------|------|
| 大图 Canvas 性能 | 限制编辑画布为 2048px 内，最终合成用 Rust |
| Canvas.toBlob 内存 | 现代浏览器 OK，10MP 以内无压力 |
| 字体跨平台差异 | 使用系统字体 + web safe fallback |
| 撤销栈内存 | 限制 20 步，超限后"压缩"到更早步骤 |
