# Medix 设计语言规范

> 本文档定义 Medix 前端视觉设计的标准。所有 UI 代码应遵循此规范，
> 确保无论是 AI 还是人类开发者，产出的界面风格一致。

---

## 1. 色彩系统

### 设计哲学

- **暗色模式**：Lightroom 风格的冷调深灰，微蓝底调，层次分明
- **亮色模式**：Apple Photos 风格的暖白，柔和不刺眼
- **语义色是唯一的颜色来源**——绝不在组件中硬编码 hex/rgb 颜色

### 基础色阶

| 变量 | 暗色值 | 亮色值 | 用途 |
|------|--------|--------|------|
| `--color-bg-primary` | `#1a1a1e` | `#fbfbfd` | 页面底色（最底层） |
| `--color-bg-secondary` | `#242428` | `#f2f2f5` | 侧边栏、面板背景 |
| `--color-bg-tertiary` | `#2e2e34` | `#e8e8ed` | 次级区域背景 |
| `--color-bg-elevated` | `#36363d` | `#ffffff` | 卡片、对话框浮层 |
| `--color-bg-hover` | `#3a3a42` | `#eaeaef` | hover 交互反馈 |
| `--color-bg-overlay` | `rgba(0,0,0,0.55)` | `rgba(0,0,0,0.25)` | 模态遮罩 |

| 变量 | 暗色值 | 亮色值 | 用途 |
|------|--------|--------|------|
| `--color-text-primary` | `#efeff1` | `#1d1d1f` | 标题、正文主色 |
| `--color-text-secondary` | `#a0a0ab` | `#6e6e78` | 次要文字 |
| `--color-text-muted` | `#6b6b78` | `#9898a2` | 辅助/占位文字 |

| 变量 | 暗色值 | 亮色值 | 用途 |
|------|--------|--------|------|
| `--color-border` | `#38383e` | `#d4d4db` | 默认边框 |
| `--color-border-light` | `#2f2f35` | `#e3e3e9` | 浅边框（卡片内） |

### 语义色（跨主题一致）

| 变量 | 暗色值 | 亮色值 | 用途 |
|------|--------|--------|------|
| `--color-accent` | `#5b9aff` | `#0071e3` | 主强调色、选中态、链接 |
| `--color-accent-hover` | `#7aafff` | `#0077ed` | 强调色 hover |
| `--color-accent-soft` | `rgba(91,154,255,0.13)` | `rgba(0,113,227,0.1)` | 强调色浅背景（tag 等） |
| `--color-accent-soft-hover` | `rgba(91,154,255,0.22)` | `rgba(0,113,227,0.18)` | 强调色浅背景 hover |
| `--color-success` | `#34d399` | `#30b47b` | 成功/确认/多选 |
| `--color-success-soft` | `rgba(52,211,153,0.12)` | `rgba(48,180,123,0.12)` | 成功浅背景 |
| `--color-warning` | `#fbbf24` | `#d9992e` | 警告/注意 |
| `--color-warning-soft` | `rgba(251,191,36,0.12)` | `rgba(217,153,46,0.12)` | 警告浅背景 |
| `--color-danger` | `#f87171` | `#d32f2f` | 危险/删除 |
| `--color-danger-soft` | `rgba(248,113,113,0.1)` | `rgba(211,47,47,0.1)` | 危险浅背景 |

### 使用规则

```tsx
// 正确：用 CSS 变量
className="text-[var(--color-accent)]"
className="bg-[var(--color-bg-elevated)]"
className="border-[var(--color-border)]"

// 错误：硬编码颜色
className="text-blue-400"        // 禁止
className="bg-[#36363d]"         // 禁止
className="border-red-800/50"    // 禁止，应改为 border-[var(--color-danger)]/30
```

---

## 2. 字体排印

### 字体

| 角色 | 字体 | 加载方式 |
|------|------|----------|
| 正文 | **Inter** (400/500/600/700) | Google Fonts CDN，`styles.css` 顶部 `@import` |
| 等宽 | SF Mono / Cascadia Code / ui-monospace | 系统字体 fallback |

### 字号阶梯

| 字号 | Tailwind | 用途 |
|------|----------|------|
| 11px | `text-[11px]` | **最小可见字号**——标签计数、辅助文字 |
| 12px | `text-xs` | 正文、标签、按钮、元数据 |
| 14px | `text-sm` | 导航项、标题栏、正文强调 |
| 16px | `text-base` | （暂未使用，预留） |
| 20px | `text-xl` | 页面标题 |
| 24px | `text-2xl` | 设置页标题 |

**规则**：任何地方不得使用小于 11px 的字号。之前的 `text-[9px]` / `text-[10px]` 已全部升级。

### 字重

| 场景 | 字重 | Tailwind |
|------|------|----------|
| 导航激活项 | 600 | `font-semibold` |
| 导航非激活 | 500 | `font-medium` |
| 卡片标题 | 500 | `font-medium` |
| 正文/输入框 | 400 | `font-normal`（默认） |
| 按钮 | 500 | `font-medium` |

---

## 3. 圆角

| 尺寸 | Tailwind | 场景 |
|------|----------|------|
| 4px | `rounded` | 按钮、输入框、标签 pill |
| 8px | `rounded-lg` | 导航项、小面板 |
| 12px | `rounded-xl` | **缩略图卡片**、对话框、搜索栏 |
| 9999px | `rounded-full` | Tag pill、进度条、状态点 |

---

## 4. 阴影

| 深度 | Tailwind | 场景 |
|------|----------|------|
| 浅 | `shadow-sm` | 卡片默认态——微妙的层次提示 |
| 中 | `shadow-lg` | 卡片 hover 上浮、Toast |
| 深 | `shadow-2xl` | 对话框、确认框、右键菜单 |
| +dark | `shadow-black/20` | 暗色模式卡片 hover 专用叠加 |

卡片 hover 浮起公式：
```
hover:shadow-lg hover:shadow-black/20 hover:-translate-y-0.5
```

---

## 5. 动效

### 可用动画（Tailwind 自定义）

| 动画 | Tailwind | 场景 |
|------|----------|------|
| 淡入 | `animate-fade-in` | 遮罩、面板出现 |
| 淡入上移 | `animate-fade-in-up` | Toast、通知 |
| 缩放入场 | `animate-scale-in` | 对话框、确认框、右键菜单 |
| 闪烁骨架 | `animate-shimmer` | 加载占位卡片 |

### 过渡

大部分交互使用 `transition-colors duration-150` 或 `duration-200`。

卡片使用 `transition-all duration-200 ease-out`（包含 transform 和 shadow）。

主题切换由 `styles.css` 中的 `html { transition: background-color 0.3s ease; }` 全局处理。

图片加载统一模式：
```
opacity-0 scale-95 blur-sm → opacity-100 scale-100 blur-0
transition-all duration-500 ease-out
```

### 按钮按下反馈

主按钮加 `active:scale-[0.97]`，点击瞬间微缩。

---

## 6. 组件模式

### 按钮

```tsx
// 主按钮
className="rounded bg-[var(--color-accent)] px-3 py-1.5 text-xs font-medium text-white
           transition-colors hover:bg-[var(--color-accent-hover)] disabled:opacity-50
           active:scale-[0.97]"

// 次级按钮
className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)]
           px-2 py-1 text-xs text-[var(--color-text-secondary)]
           transition-colors hover:bg-[var(--color-bg-hover)] active:scale-[0.97]"

// 危险按钮
className="rounded border border-[var(--color-danger)]/20 bg-[var(--color-danger-soft)]
           px-3 py-1.5 text-xs text-[var(--color-danger)]
           transition-colors hover:bg-[var(--color-danger-soft)]/80 active:scale-[0.97]"
```

### 输入框

```tsx
className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)]
           px-2 py-1.5 text-sm text-[var(--color-text-primary)] outline-none
           placeholder:text-[var(--color-text-muted)]
           focus:border-[var(--color-accent)]"
```

### 卡片（缩略图）

```tsx
// 默认态
"rounded-xl bg-[var(--color-bg-elevated)] shadow-sm overflow-hidden
 transition-all duration-200 ease-out"

// hover 态
"hover:shadow-lg hover:shadow-black/20 hover:-translate-y-0.5"

// 选中态
"ring-2 ring-[var(--color-accent)] ring-offset-2 ring-offset-[var(--color-bg-primary)]"

// 多选态
"ring-2 ring-[var(--color-success)] ring-offset-2 ring-offset-[var(--color-bg-primary)]"

// 勾选框：右上角，平时隐藏，hover/selectionMode 显示，毛玻璃背景
"absolute right-2 top-2 z-10 transition-all duration-150
 opacity-0 group-hover:opacity-100   (或 selectionMode ? opacity-100 : ...)

 // 勾选按钮本身
 flex h-6 w-6 items-center justify-center rounded-md border-2 backdrop-blur-sm
 border-white/70 bg-white/15 hover:border-white
 (选中时) border-[var(--color-accent)] bg-[var(--color-accent)] text-white"
```

### 对话框 / 模态框

```tsx
// 遮罩
"fixed inset-0 z-50 flex items-center justify-center
 bg-[var(--color-bg-overlay)] animate-fade-in"

// 内容卡片
"w-80 rounded-xl bg-[var(--color-bg-elevated)] border border-[var(--color-border)]
 shadow-2xl animate-scale-in p-5"

// 标题
"text-sm font-semibold text-[var(--color-text-primary)] mb-2"

// 描述
"text-xs text-[var(--color-text-secondary)] mb-5"
```

### 右键菜单

```tsx
// 容器
"min-w-[140px] rounded-xl border border-[var(--color-border)]
 bg-[var(--color-bg-elevated)]/95 backdrop-blur-xl py-1.5
 shadow-2xl shadow-black/30 animate-scale-in"

// 菜单项
"block w-full px-3 py-2 text-left text-xs text-[var(--color-text-secondary)]
 transition-colors hover:bg-[var(--color-bg-hover)] hover:text-[var(--color-text-primary)]
 first:rounded-t-lg last:rounded-b-lg"

// 危险项
"hover:bg-[var(--color-danger-soft)] hover:text-[var(--color-danger)]"
```

### 导航激活态

```tsx
// 侧边栏激活：左侧强调竖线 + 浅色 accent 背景
"border-l-[3px] border-[var(--color-accent)]
 bg-[var(--color-accent-soft)] text-[var(--color-accent)]"

// 非激活：透明左边界防止布局跳动
"border-l-[3px] border-transparent
 text-[var(--color-text-secondary)]
 hover:bg-[var(--color-bg-hover)] hover:text-[var(--color-text-primary)]"
```

---

## 7. Native-Feel 规则

这些规则确保 WebView 应用看起来不像网页：

1. **禁止文字选中** — `body { user-select: none }`，仅输入框可选中
2. **禁止图片拖拽** — `img { -webkit-user-drag: none }`
3. **鼠标点击不显聚焦环** — `*:focus:not(:focus-visible) { outline: none }`
4. **键盘导航保留聚焦环** — `*:focus-visible { outline: 2px solid var(--color-accent) }`
5. **不用浏览器原生对话框** — 禁止 `window.confirm()`、`alert()`、`prompt()`，统一用 `ConfirmDialog` 组件
6. **不用 `<select>` 原生外观** — 表格/详情面板中如果用了 `<select>`，需要用 CSS 定制
7. **不自定义滚动条** — 删除了 `::-webkit-scrollbar` 样式，让 WebView2 原生覆盖滚动条接管
8. **拖动区域** — 标题栏使用 CSS `-webkit-app-region: drag`，按钮区域 `no-drag`
9. **字体抗锯齿** — `body { -webkit-font-smoothing: antialiased }` 适配 Windows 渲染

---

## 8. 间距

| 密度 | Tailwind | 场景 |
|------|----------|------|
| 紧凑 | `space-y-1` / `gap-1.5` | 详情面板元数据、标签列表 |
| 正常 | `space-y-3` / `p-4` / `gap-3` | 卡片网格、面板内容 |
| 宽松 | `p-6` / `space-y-6` | 设置页面、独立页面 |

侧边栏：
- 导航项垂直间距：`py-2.5`
- 分区标题上方间距：`pt-5`
- 分区标题下方间距：`pb-1.5`

---

## 9. 新增组件时 Checklist

- [ ] 所有颜色使用 `var(--color-*)`，不硬编码 hex/rgb
- [ ] 最小字号 ≥ 11px
- [ ] 按钮有 hover + active 状态
- [ ] 浮层元素（对话框、菜单）有入场动画（`animate-scale-in` / `animate-fade-in`）
- [ ] 图片加载有 fade+blur 过渡
- [ ] 不用 `window.confirm()`，用 `ConfirmDialog`
- [ ] 卡片/浮层用 `rounded-xl`，按钮/输入框用 `rounded`
- [ ] 主题切换下表现正常（暗色/亮色都用 CSS 变量自动适配）
