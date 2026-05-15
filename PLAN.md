# Medix - 媒体数据集管理与压缩软件

## 项目概述

一个本地优先的媒体数据集管理工具，支持图片导入、AI自动标签、详细标注、多格式变体、搜索过滤和浏览器插件集成。

## 技术栈

| 层级 | 技术 |
|------|------|
| 桌面框架 | Tauri v2 (Rust + Webview) |
| 前端 | React 19 + TypeScript + Tailwind CSS |
| 数据库 | SQLite + rusqlite |
| 图像处理 | Rust `image` + `kamadak-exif` |
| AI推理 | llama.cpp `llama-server` (OpenAI 兼容 HTTP API) |
| 搜索索引 | SQLite FTS5 |
| 浏览器插件 | Chrome Extension Manifest V3 |

---

## Phase 1: 项目骨架与基础架构

**目标**: 可运行的空壳应用，能展示主窗口和基础导航

### 任务
1. 初始化 Tauri v2 项目 (`npm create tauri-app@latest`)
2. 配置前端路由 (React Router) 和全局状态管理 (Zustand)
3. 设计并实现基础布局：侧边栏导航 + 主内容区
4. 配置 Tailwind CSS 和基础UI组件库 (shadcn/ui 或自研)
5. 配置 SQLite 数据库连接和迁移系统

### 验证标准
- [x] `cargo tauri dev` 能正常启动，显示主窗口
- [x] 侧边栏可在"全部媒体"/"标签"/"设置"间切换
- [x] 数据库文件在 `AppData/Roaming/com.bronze107.medix/medix.db` 成功创建
- [x] 前端可调用 Tauri command `greet(name)` 并返回 `"Hello, {name}!"`

---

## Phase 2: 媒体导入与基础浏览

**目标**: 拖入图片能导入、生成缩略图、在网格中浏览

### 任务
1. 实现拖放导入 (DropZone 组件)
   - 支持多文件拖拽
   - 支持文件夹拖拽导入（递归遍历子目录）
   - 文件类型验证 (jpg/png/webp/gif/bmp)
2. Rust 后端：文件处理管道
   - 复制/移动文件到应用数据目录 (`%APPDATA%/Medix/library/`)
   - 生成 ULID 作为主键
   - 提取 EXIF 信息 (宽高、创建时间、修改时间)
3. 缩略图生成服务
   - 生成 256x256 和 512x512 的 JPEG 缩略图
   - 异步队列处理，不阻塞UI
4. 网格浏览界面
   - 虚拟滚动 (react-window 或自研) 支持万级图片
   - 缩略图懒加载
5. 基础详情面板
   - 点击缩略图显示右侧详情：尺寸、文件大小、创建时间

### 验证标准
- [x] 拖入 10 张图片，全部出现在网格中，缩略图正常显示
- [x] 数据库 `media` 表有 10 条记录，字段完整
- [x] `library/` 和 `thumbnails/` 目录结构正确
- [x] 导入 1000 张图片，滚动流畅无卡顿 (虚拟滚动生效)
- [x] 支持按 `created_at` / `modified_at` / `imported_at` 排序
- [x] 拖入文件夹，递归导入其中所有支持的图片

---

## Phase 3: 标签系统与AI自动标注

**目标**: 本地优先 + 云端后备的 AI 自动 caption/tag/embedding

**架构**: Medix 管理 `llama-server` 子进程，通过 OpenAI 兼容 HTTP API 调用 VLM 和 Embedding

### 任务
1. [x] 数据库：标签相关表
   - [x] `tags`、`media_tags` 表（`confidence`/`source` 字段支持 AI）
   - [x] `embeddings` 表 (media_id, model, content_type, vector)
   - [x] `settings` 表（AI 模式/路径/云端配置持久化）
   - [ ] FTS5 虚拟表（推迟到 Phase 6）
2. [x] AI 推理引擎（本地优先，云端后备）
   - [x] 本地 VLM：`llama-server` + MiniCPM-V 2.6 (~1GB Q4 GGUF)
     - [x] OpenAI `/v1/chat/completions` 生成 dense caption + 结构化 tags
     - [x] `std::sync::mpsc` 异步推理队列，导入时自动触发
   - [x] 本地 Embedding：共用 VLM 模型通过 `/v1/embeddings` 向量化
     - [x] caption 和 tags 分别向量化
     - [x] blob 存储 f32 向量到 `embeddings` 表
     - [x] `--embeddings --pooling mean` 参数启用
     - [ ] 后续优化：支持专用 embedding 模型（nomic-embed-text）独立实例（见 Phase 6）
   - [x] llama-server 子进程生命周期管理（启动/健康检查/关闭）
   - [x] `--mmproj` 视觉投影器支持
   - [x] 自动检测二进制路径 + 扫描 models/ 目录下 GGUF/mmproj 文件
   - [ ] 云端后备：Claude / OpenAI / Qwen3.5 API（用户配置 API key）
     - [ ] 本地模型加载失败时自动降级
     - [ ] 用户可手动选择"云端模式"获取更高质量结果
3. [x] 标签管理UI
   - [x] Tags 页面：创建 / 删除 / 重命名标签
   - [x] 图片详情面板显示标签列表 + 添加 / 移除标签
   - [x] 批量打标签 (多选图片)
   - [x] AI 标签蓝色 badge 区分显示
   - [x] AI 描述独立区域 + "采纳为手动描述"按钮
   - [x] 详情页底部显示 embedding 向量状态
4. [x] 标签过滤
   - [x] 顶部搜索框支持 `tag:cat dog` 语法（多标签交集筛选）
   - [ ] 标签云侧边栏（推迟）
5. [x] 设置页面
   - [x] AI 模式选择：自动/本地/云端
   - [x] llama-server 启停控制 + 端口配置
   - [x] 二进制路径/模型/mmproj 自动检测 + 下拉选择
   - [x] 线程数/GPU层数/上下文大小配置
   - [x] 云端 API Key 输入（预留）
   - [x] GGUF 模型文件扫描列表

### 验证标准
- [x] 导入一张图片，自动生成 caption 和 tags
- [x] AI 生成的 tags 带 `source=ai` 标记，蓝色 badge 与人工标签区分显示
- [x] AI caption 显示在"描述"标签页的 AI 描述区域
- [x] Embedding 向量成功存储（详情页显示维度）
- [x] 断网状态下本地 llama-server 仍能正常工作
- [x] 删除图片时，关联的 tags / captions / embeddings 级联删除（外键 ON DELETE CASCADE）
- [x] 搜索框输入 `tag:cat` 只显示带 cat 标签的图片
- [x] 选择 5 张图片，批量添加 `favorite` 标签，全部生效

---

## Phase 4: 变体系统 (Variants)

**目标**: 支持同一图片的多格式、多分辨率变体

### 任务
1. [x] 数据库：`variants` 表 + migration
2. [x] 变体生成引擎
   - [x] 格式转换：JPEG、PNG (WebP/AVIF 推迟 — `image` crate v0.25 无 encoder)
   - [x] 分辨率缩放：等比缩放 (Lanczos3)
   - [x] 质量档位：JPEG 可调 quality，PNG 无损
   - [x] 懒生成：先查数据库+文件系统，存在则直接返回，不重新编码
3. [x] 变体管理UI
   - [x] 详情面板标签页切换："详情" / "变体"
   - [x] 变体标签页：列出所有变体（格式、尺寸、文件大小）
   - [x] 每个变体可删除（同步删除磁盘文件和数据库记录）
   - [ ] 预览对比：原图 vs 变体并排对比（推迟）
4. [x] 变体预设系统
   - [x] 内置 3 个预设：Web分享 (JPEG, 1080px, Q75) / 打印 (PNG, 2048px, Q95) / 训练数据集 (JPEG, 512px, Q85)
   - [ ] 用户自定义预设 (JSON 配置)（推迟）

### 验证标准
- [x] 对一张 4MB 的 PNG 生成 JPEG/quality-75 变体，体积显著缩小
- [x] 同一张图片请求同一变体两次，第二次直接返回缓存，不重新编码
- [x] 删除变体后，对应文件从磁盘和数据库同时删除
- [ ] 导出时选择 "Web分享" 预设批量生成并打包（Phase 7）

---

## Phase 5: 文本描述标注 (Captions)

**目标**: 支持对单张图片添加多条文本描述标注

### 任务
1. [x] 数据库：`captions` 表 + migration `0004_captions`
   - `id`, `media_id`, `text`, `created_at`, `updated_at`
   - 一张图片可有多条 caption，ON DELETE CASCADE
2. [x] 描述编辑UI
   - [x] 详情面板新增 "描述" 标签页（与 "详情"/"变体" 并列）
   - [x] 文本输入框支持多行输入（Ctrl+Enter 保存）
   - [x] 列出当前图片的所有描述条目
   - [x] 每条描述可编辑、删除
3. [ ] 批量管理（可选，推迟）
   - [ ] 批量编辑选中图片的描述
   - [ ] 导出为纯文本 / JSON 格式

### 验证标准
- [x] 为一张图片添加描述，保存后重新打开显示正确
- [x] 编辑已有描述，内容更新成功
- [x] 删除描述后列表正确刷新
- [x] 删除图片时，关联的 captions 级联删除（外键 ON DELETE CASCADE）

---

## Phase 6: 搜索与高级过滤

**目标**: 语义搜索 + 结构化过滤 + 智能筛选器

### 任务
1. [x] 语义搜索（基于 Phase 3 的 embedding）
   - [x] 查询文本 -> llama-server embedding 向量化
   - [x] 与 `embeddings` 表中 caption/tags 向量做余弦相似度排序
   - [x] 最低相似度阈值 0.25 过滤噪声
   - [x] 与现有 `tag:` 语法共存，可混合使用
   - [ ] **后续优化**：支持专用 embedding 模型（如 nomic-embed-text）独立实例/端口
     - 当前共用 MiniCPM-V 做 embedding，语义质量不如专用模型
     - 方案：设置页支持配置第二个模型 + 端口，llama-server 启动两个实例
2. [x] 高级过滤语法
   - [x] `tag:cat dog` - 交集 / `tag:cat | dog` - 并集
   - [x] `width:>1920` / `height:800..1080` - 尺寸过滤
   - [x] `date:2024-01..2024-12` - 日期范围
   - [x] `size:<1mb` / `size:>500kb` - 文件大小
   - [x] `橘子猫`（无前缀）- 语义搜索
   - [x] 混合查询 `tag:cat 橘子猫 width:>1000`
3. [x] 保存的筛选器
   - [x] 侧边栏"已保存的筛选器"区域，点击即时应用
   - [x] 悬停显示删除按钮
   - [x] 搜索框旁"保存筛选"按钮
4. [x] 搜索前端
   - [x] SearchBar 组件：彩色 pill 标签显示活跃过滤器 + 清除按钮
   - [x] 空搜索结果友好提示 + 重置按钮
5. [ ] 全文搜索（推迟）
   - [ ] SQLite FTS5（语义搜索已覆盖 caption/tags 文本匹配，优先级降低）
   - [ ] 搜索建议 (debounce 200ms)

### 验证标准
- [x] 语义搜索 "一只橘猫" 返回包含橘猫的图片（需有相关 caption/tags 的 embedding）
- [x] 搜索 `tag:cat` 过滤正确
- [x] 混合查询 `tag:cat 橘子猫 width:>1000` 组合过滤正确
- [x] 空搜索结果时显示友好提示和重置按钮
- [x] 保存筛选器后，重启应用仍可用

---

## Phase 7: 数据集导入导出

**目标**: 支持数据集打包导出（图片 + 标注），多种格式和变体选择

### 任务
1. 导出向导
   - 选择导出范围：当前筛选结果 / 选中项 / 全部
   - 选择包含的变体：原图 / 缩略图 / 特定预设
   - 选择导出模式：
     - **复制到目录**：图片 + 每图同名 `.txt` caption 文件 + `metadata.jsonl`
     - **ZIP 打包**：同上但压缩为 ZIP
   - 前端进度条（Tauri 事件推送进度）
2. 标注导出格式
   - 用户可选择要导出的 caption：仅手动 / 仅 AI / 全部
   - 每张图片导出同名 `.txt`（纯文本）和 `.json`（结构化元数据）
   - `.txt`：内容为所选 caption 文本
   - `.json`：`{filename, caption, tags: [...], width, height}`（单条 caption 为对象，多条为数组）
   - 不实现 COCO/YOLO（需要 bounding box，当前无区域标注功能）
3. ZIP 导入
   - 解压到临时目录，遍历支持的图片文件走正常导入流程
   - 可选择复制到库或仅建立引用

### 验证标准
- [ ] 导出 50 张图到目录，每张有对应同名 `.txt` 和 `.json`，格式正确
- [ ] 选择"仅手动 caption"导出，不包含 AI caption
- [ ] ZIP 导出后解压，文件结构完整
- [ ] 导入 ZIP 后，媒体列表正确显示所有图片
- [ ] 导出进度条在导出 1000 张图片时线性增长，不卡死
- [ ] 选择 "Web分享" 变体导出，生成的是缩放后的 JPEG 而非原图

---

## Phase 8: 浏览器插件

**目标**: 浏览器右键一键添加图片到库

### 任务
1. Chrome Extension 基础
   - Manifest V3 配置
   - 右键菜单：`添加到 Medix`
   - 图标和基础弹窗 (显示连接状态)
2. 本地通信协议
   - Tauri 应用启动 localhost HTTP 服务 (port 8765)
   - CORS 配置允许扩展访问
   - 简单 REST API: `POST /api/import` `{url, page_url, alt_text}`
3. 后端下载处理
   - 收到 URL 后异步下载图片
   - 走正常导入流程 (AI标签、缩略图)
   - 系统通知提示导入结果
4. 插件设置
   - 配置 Tauri 服务端口号
   - 选择默认导入标签

### 验证标准
- [ ] Tauri 启动后，浏览器插件显示 "已连接"
- [ ] 在网页右键图片 → "添加到 Medix"，桌面应用弹出通知并显示新图片
- [ ] 关闭 Tauri 应用后，插件显示 "未连接，请启动 Medix"
- [ ] 一次导入 10 张网页图片，全部成功且带 `from_web` 标签

---

## Phase 9: 性能优化与发布准备

**目标**: 生产就绪，万级图库流畅运行

### 任务
1. 性能优化
   - 缩略图预生成改为 Worker 线程 / 后台任务
   - 大型网格使用虚拟滚动 + 回收 DOM
   - SQLite 索引优化 (覆盖索引、查询计划分析)
   - 图片解码使用流式/分块处理
2. 数据安全
   - 定期自动备份数据库
   - 导入前计算 phash，重复文件提示
   - 回收站机制 (软删除)
3. 打包发布
   - Tauri 代码签名配置
   - 自动更新 (Tauri updater)
   - Windows Installer (MSI + NSIS)

### 验证标准
- [ ] 图库 10000 张图片，冷启动到可浏览 < 3 秒
- [ ] 连续导入 1000 张图片，内存增长 < 200MB，不泄露
- [ ] 重复导入同一张图片，提示"已存在"而不是重复存储
- [ ] 卸载应用后重新安装，数据库备份可恢复

---

## 目录结构

```
Medix/
├── PLAN.md                     # 本文档
├── src/
│   ├── main.tsx                # 前端入口
│   ├── App.tsx                 # 根组件
│   ├── components/             # UI 组件
│   │   ├── Layout/
│   │   ├── Gallery/
│   │   ├── DetailPanel/
│   │   ├── DropZone/
│   │   ├── AnnotationCanvas/
│   │   ├── SearchBar/
│   │   └── TagManager/
│   ├── hooks/                  # React hooks
│   ├── stores/                 # Zustand stores
│   ├── types/                  # TypeScript 类型
│   └── lib/                    # 工具函数
├── src-tauri/
│   ├── src/
│   │   ├── main.rs             # 入口
│   │   ├── commands/           # Tauri IPC 命令
│   │   ├── db/                 # 数据库模块 (migrations + CRUD)
│   │   ├── media/              # 媒体处理 (import + thumbnail)
│   │   ├── ai/                 # AI 推理
│   │   │   ├── mod.rs          # AiQueue 异步队列
│   │   │   ├── llamacpp.rs     # OpenAI 兼容 HTTP 客户端
│   │   │   └── server.rs       # llama-server 子进程管理
│   │   ├── models/             # GGUF 模型扫描 + 自动检测
│   │   ├── settings/           # 设置键定义 + get/set helper
│   │   ├── variants/           # 变体生成
│   │   └── tag/                # Tag 结构体
│   └── Cargo.toml
├── extension/                  # 浏览器插件
│   ├── manifest.json
│   ├── background.js
│   ├── content.js
│   └── popup.html
└── models/                     # AI 模型文件 (gitignored)
```

---

## 开发流程

1. 每个 Phase 独立开发，完成后打 tag (`v0.1.0`, `v0.2.0`...)
2. 每 Phase 开始时更新本 PLAN.md，勾选已完成项
3. 关键设计决策记录到 `docs/decisions/` (ADRs)
4. Rust 核心模块必须写单元测试，前端组件写 Storybook

---

## 风险与预案

| 风险 | 影响 | 预案 |
|------|------|------|
| llama-server 启动失败 | Phase 3 阻塞 | 自动检测二进制路径、提供云端 API 降级方案 |
| 大图内存溢出 | Phase 2/4 | 限制单图最大尺寸，超大图分块处理 |
| 万级图库卡顿 | Phase 2/9 | 提前引入虚拟滚动，SQLite 分页查询 |
| 浏览器插件审核 | Phase 8 | 同时支持 Firefox + Edge 商店，提供 crx 手动安装 |
