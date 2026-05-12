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
| AI推理 | ONNX Runtime (`ort`) |
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

**目标**: 图片自动打标签，支持人工增删改，可按标签过滤

### 任务
1. [x] 数据库：标签相关表
   - [x] `tags`、`media_tags` 表（预留 `confidence`/`source` 字段供 AI 阶段使用）
   - [ ] FTS5 虚拟表（推迟到 Phase 6）
2. [ ] AI 标签推理引擎（推迟到后续阶段）
   - [ ] 集成 `wd14-tagger` ONNX 模型
   - [ ] 首次启动时自动下载模型 (~200MB) 到 `models/`
   - [ ] 异步推理队列，导入时自动触发
   - [ ] 置信度阈值过滤 (默认 0.35)
3. [x] 标签管理UI
   - [x] Tags 页面：创建 / 删除 / 重命名标签
   - [x] 图片详情面板显示标签列表 + 添加 / 移除标签
   - [x] 批量打标签 (多选图片)
   - [ ] AI标签区分显示（等 AI 阶段）
4. [x] 标签过滤
   - [x] 顶部搜索框支持 `tag:cat dog` 语法（多标签交集筛选）
   - [ ] 标签云侧边栏（推迟）

### 验证标准
- [ ] 导入一张动漫图片，自动出现 `1girl`, `solo`, `long_hair` 等标签（AI 阶段）
- [x] 删除一个标签后，该图片不再显示此标签
- [x] 搜索框输入 `tag:cat` 只显示带 cat 标签的图片
- [x] 选择 5 张图片，批量添加 `favorite` 标签，全部生效
- [x] 删除标签 `test`，所有关联的 media_tags 记录被级联删除

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
1. 数据库：`captions` 表
   - `id`, `media_id`, `text`, `created_at`, `updated_at`
   - 一张图片可有多条 caption
2. 描述编辑UI
   - 详情面板新增 "描述" 标签页（与 "详情"/"变体" 并列）
   - 文本输入框支持多行输入
   - 列出当前图片的所有描述条目
   - 每条描述可编辑、删除
3. 批量管理（可选）
   - 批量编辑选中图片的描述
   - 导出为纯文本 / JSON 格式

### 验证标准
- [ ] 为一张图片添加描述，保存后重新打开显示正确
- [ ] 编辑已有描述，内容更新成功
- [ ] 删除描述后列表正确刷新
- [ ] 删除图片时，关联的 captions 级联删除

---

## Phase 6: 搜索与高级过滤

**目标**: 强大的搜索能力，支持多维度复合查询

### 任务
1. 全文搜索
   - SQLite FTS5 实现标签和文件名搜索
   - 前端搜索框支持实时建议 (debounce 200ms)
2. 高级过滤语法
   - `tag:cat dog` - 包含 cat 和 dog
   - `width:>1920` - 宽大于1920
   - `date:2024-01-01..2024-12-31` - 日期范围
   - `size:<1mb` - 文件大小
   - 支持逻辑组合：`tag:cat AND (width:>1000 OR height:>1000)`
3. 保存的筛选器
   - 用户可将常用筛选条件保存为智能文件夹
   - 显示在侧边栏，点击即时应用

### 验证标准
- [ ] 搜索 `tag:cat` 在 1000 张图库中返回结果 < 100ms
- [ ] 组合查询 `tag:landscape width:>1920` 结果正确
   - 单独 `tag:landscape` 100 张，`width:>1920` 50 张，组合后 30 张
- [ ] 保存筛选器后，重启应用仍可用
- [ ] 空搜索结果时显示友好提示和重置按钮

---

## Phase 7: 导入导出与压缩

**目标**: 支持数据集打包导出，多种格式和变体选择

### 任务
1. 导出向导
   - 选择导出范围：当前筛选结果 / 选中项 / 全部
   - 选择包含的变体：原图 / 缩略图 / 特定预设
   - 选择标注格式：无 / COCO JSON / YOLO txt / 自定义
2. 压缩打包
   - ZIP / TAR.GZ 输出
   - 显示进度条 (大文件导出可能耗时)
   - 支持分卷压缩 (大数集)
3. 导入兼容
   - 支持从 ZIP 导入 (含目录结构保留)
   - 支持导入外部目录 (建立引用或复制)

### 验证标准
- [ ] 导出 50 张图 + COCO 标注，ZIP 文件可正常解压且 JSON 有效
- [ ] 导出的 YOLO 格式可直接被 `yolov8 train` 识别
- [ ] 导入 ZIP 后，媒体列表正确显示所有图片
- [ ] 导出进度条在导出 1000 张图片时线性增长，不卡死

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
│   │   ├── db/                 # 数据库模块
│   │   ├── media/              # 媒体处理
│   │   ├── ai/                 # AI 推理
│   │   ├── variants/           # 变体生成
│   │   ├── export/             # 导入导出
│   │   └── server/             # HTTP 服务 (浏览器插件)
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
| ONNX 模型加载失败 | Phase 3 阻塞 | 提供云端 API 降级方案 (需配置 key) |
| 大图内存溢出 | Phase 2/4 | 限制单图最大尺寸，超大图分块处理 |
| 万级图库卡顿 | Phase 2/9 | 提前引入虚拟滚动，SQLite 分页查询 |
| 浏览器插件审核 | Phase 8 | 同时支持 Firefox + Edge 商店，提供 crx 手动安装 |
