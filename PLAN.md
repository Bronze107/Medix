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
   - [x] **2026-06-02** FTS5 虚拟表（migration 0017_fts5，搜索集成）
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

## Phase 4: 版本控制系统 (Version Control)

**目标**: 同一原图支持多个衍生版本（内部生成 + 外部导入），带自定义标签和时间线

### 任务
1. [x] 数据库：`variants` 表 + migration `0003_variants` + `0011_variant_versioning`
   - [x] `label` 字段（用户自定义版本名）、`source` 字段（`generated` / `imported`）
   - [x] 旧数据自动回填 label（Web分享/打印/训练数据集）
2. [x] 版本生成引擎
   - [x] 格式转换：JPEG、PNG (WebP/AVIF 推迟 — `image` crate v0.25 无 encoder)
   - [x] 分辨率缩放：等比缩放 (Lanczos3)，自定义最大宽/高
   - [x] 质量：JPEG 可调 quality 1-100，PNG 无损
   - [x] 3 个内置预设作为模板（Web分享/打印/训练数据集），一键填充参数
3. [x] 外部版本导入
   - [x] `variant_import` 命令：复制外部文件到 variants/ 目录 → 创建 DB 记录
   - [x] 支持通过文本路径或文件选择器添加版本
   - [x] `source="imported"` 与生成版本区分，绿色 "导入" badge
4. [x] 版本管理UI
   - [x] 详情面板标签页："版本"（原名"变体"）
   - [x] 版本卡片：缩略图 + 自定义标签 + 来源 badge + 格式/尺寸/文件大小
   - [x] 每个版本可删除（同步删除磁盘文件和数据库记录）
   - [x] 预览对比：Lightbox 并排对比 + 滑块叠加模式
5. [x] 版本生成表单
   - [x] 预设模板按钮（填充参数，不直接生成）
   - [x] 自定义参数：名称、格式、最大宽/高、质量
   - [x] "生成版本"按钮

### 验证标准
- [x] 自定义参数生成版本，格式/尺寸/质量正确
- [x] 外部文件导入为版本，label 取自文件名，source="imported"
- [x] 旧变体数据迁移后正常显示，label 回填正确
- [x] 删除版本后，对应文件从磁盘和数据库同时删除
- [x] Lightbox 对比模式下原图和版本同尺寸渲染
- [x] 版本卡片显示缩略图 + 标签 + 来源 badge

---

## Phase 5: 文本描述标注 (Captions)

**目标**: 支持对单张图片添加多条文本描述标注

### 任务
1. [x] 数据库：`captions` 表 + migration `0004_captions`
   - `id`, `media_id`, `text`, `created_at`, `updated_at`
   - 一张图片可有多条 caption，ON DELETE CASCADE
2. [x] 描述编辑UI
   - [x] 详情面板新增 "描述" 标签页（与 "详情"/"版本" 并列）
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
5. [x] **2026-06-02** 全文搜索
   - [x] SQLite FTS5 — migration 0017_fts5，unicode61 tokenizer
   - [x] caption/tag CRUD 后自动增量同步 search_text
   - [x] 启动时检查 FTS 空则全量重建已有数据
   - [x] 与语义搜索结果合并（OR），设置页可独立开关
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
1. [x] 导出向导
   - [x] 选择导出范围：选中项 / 当前筛选结果
   - [x] 选择包含的变体：原图 + Web分享/打印/训练数据集
   - [x] 选择导出模式：复制到目录 / ZIP 打包
   - [x] 前端进度条（Tauri event `export-progress` 推送）
2. [x] 标注导出格式
   - [x] 用户可选择要导出的 caption：仅手动 / 仅 AI / 全部
   - [x] 每张图片导出同名 `.txt`（纯文本）和 `.json`（结构化元数据）
   - [x] `.txt`：内容为所选 caption 文本
   - [x] `.json`：`{filename, caption, tags, width, height}`（单条 caption 为对象，多条为数组）
3. [x] ZIP 导入
   - [x] 后端 `import_zip` 命令：解压到临时目录 → 遍历图片 → 走正常导入流程
   - [ ] 前端导入按钮（后续补充文件选择对话框）

### 验证标准
- [x] 导出到目录，每张有对应同名 `.txt` 和 `.json`，格式正确
- [x] 选择"仅手动 caption"导出，不包含 AI caption
- [x] ZIP 导出后解压，文件结构完整
- [x] 导出进度条实时更新
- [x] 选择变体导出，生成的是缩放后的版本而非原图

---

## Phase 8: 浏览器插件 + 媒体来源追踪

**目标**: 浏览器右键一键添加图片到库，记录图片来源

**实现**: `tiny_http` 本地 HTTP 服务 (8765) + Chrome Extension Manifest V3

### 任务
1. [x] 数据库：media 表增加来源字段 (migration 0008)
   - [x] `source_url TEXT`（图片 URL）、`page_url TEXT`（所在页面 URL）、`source TEXT`（`web` / `local` / `zip`）
   - [x] 前端详情面板显示来源信息（可点击 URL 打开浏览器）
2. [x] 本地 HTTP 服务
   - [x] Tauri 启动时自动启动 `tiny_http` 服务（端口可配置，默认 8765）
   - [x] `POST /api/import` 接收 `{url, page_url, alt_text}` → 下载 → 导入 → 缩略图 + AI
   - [x] `GET /api/health` 健康检查
   - [x] 导入完成后 Tauri event `remote-import` 推送前端自动刷新
3. [x] Chrome Extension
   - [x] Manifest V3，右键菜单 `添加到 Medix`（contexts: image）
   - [x] 弹窗显示连接状态（绿/红点）+ 端口配置
   - [x] 3 秒轮询健康检查自动更新状态
4. [ ] 剪贴板导入（推迟）
   - [ ] Ctrl+V 粘贴网页复制的图片

### 验证标准
- [x] Tauri 启动后，浏览器插件显示 "已连接"
- [x] 在网页右键图片 → "添加到 Medix"，应用自动刷新并显示新图片
- [x] 图片详情面板显示来源 URL，可点击打开
- [x] 关闭 Tauri 应用后，插件显示 "未连接，请启动 Medix"
- [x] `source_url` / `page_url` / `source` 字段完整

---

## Phase 9: 性能优化与发布准备

**目标**: 生产就绪，万级图库流畅运行

**性能审计日期**: 2026-05-30（基于 3000-5000 张图片规模分析）

---

### 规模预测

| 图片数 | 元数据 | 缩略图磁盘 | 启动加载 | 搜索 | AI 管道 |
|--------|--------|-----------|---------|------|---------|
| 1,000 | ~0.5MB | ~200MB | 快 (<1s) | 快 | 可行 |
| **5,000** | **~2.5MB** | **~1GB** | **可感知 (2-4s)** | **可感知** | **可行** |
| 10,000 | ~5MB | ~2GB | 明显慢 (5-10s) | 慢 | 队列积压 |
| 50,000+ | ~25MB | ~10GB | 不可接受 | 不可接受 | 不可行 |

---

### 任务

#### P0 — 数据库索引（阻塞级，缺少导致全表扫描）

> 当前缺失索引导致几乎所有查询都是全表扫描。这三项改动量小（各一行 SQL），但收益巨大。

- [x] **`sha256` 索引** — 每次导入都全表扫描查重。5000 张图导入 100 张 = 500,000 次比较
  - 位置：`src-tauri/src/db/mod.rs`，`media_get_by_sha256` 的 `WHERE sha256 = ?1`
- [x] **`deleted_at` 索引** — 几乎所有查询都过滤 `WHERE deleted_at IS NULL`，无索引 = 全表扫描
  - 影响 10+ 查询：`list_media`、`media_list_trash`、`media_search_by_tags`、`media_query_filtered`、`media_list_by_collection`、`media_find_similar`、`media_get_by_sha256`、`media_get_batch` 等
- [x] **`embeddings.model` 索引** — 每次语义搜索扫描整个 embeddings 表加载所有向量到内存
  - 位置：`src-tauri/src/db/mod.rs`，`embedding_get_all_by_model` 的 `WHERE model = ?1`
  - 5000 张 × 768 维 × 4 bytes ≈ 15MB per search（无索引时）

#### P0 — 媒体列表分页（前端全量加载 → 首屏按需）

> 当前 `media_list` / `media_search` / `media_list_by_collection` 无 LIMIT，每次加载全部行。
> 同时 `resolve_thumb_paths` 对每条记录做 2× `stat()` 检查（`_256.jpg` + `_512.jpg`）。
> 5000 张 = 10000 次文件系统调用 + 全量 IPC 序列化。

- [x] **后端加分页**：`media_list` / `media_search` / `media_list_by_collection` SQL 添加 `LIMIT ? OFFSET ?`
  - 位置：`src-tauri/src/db/mod.rs`（`list_media` 约第 610 行、`media_query_filtered` 等）
- [x] **前端窗口化加载**：虚拟滚动 + 按需加载下一页（类似无限滚动）
  - 位置：`src/components/AllMedia/AllMedia.tsx`，`loadMedia` 函数

#### P1 — 缩略图批量 IPC（200 次往返 → 1 次）

> 网格视图 ~200 个可见卡片，每个卡片独立调用 `invoke("media_thumbnail", { id })` → 200 次 Tauri IPC 往返。
> 虽然有前端 Map 缓存，但首次加载/切换视图时仍然全部重新请求。

- [x] **批量命令**：新增 `media_thumbnail_batch(ids: Vec<String>)` 一次返回所有路径
  - 位置：`src-tauri/src/commands/thumbnail.rs`
- [x] **前端批量调用**：`useThumbnail` hook 改为批量预加载模式，或 Gallery 层统一请求
  - 位置：`src/hooks/useThumbnail.ts`、`src/components/Gallery/Gallery.tsx`
- [x] **统一缩略图缓存**：合并 Gallery 和 TableView 各自维护的独立 `Map`，提到全局 hook
  - 位置：`src/hooks/useThumbnail.ts` + `src/components/TableView/TableView.tsx:38`

#### P1 — 批量操作事务包装（N 次 fsync → 1 次）

> `media_tag_add_batch`、`collection_add_batch`、`caption_create_batch`、`media_empty_trash` 等
> 逐个遍历 ID，每次独立的 DB 连接 + 自动提交。批量打 100 个标签 = 100 次 fsync。

- [x] `media_tag_add_batch` 单连接 + `BEGIN/COMMIT` 包裹所有 INSERT
  - 位置：`src-tauri/src/db/mod.rs` 约第 861 行
- [x] `collection_add_batch` 同上
  - 位置：`src-tauri/src/db/mod.rs` 约第 419 行
- [x] `caption_create_batch` 使用单个 INSERT 多 VALUES
  - 位置：`src-tauri/src/commands/caption.rs` 约第 38 行

#### P2 — DB 连接池（消除 per-call Connection::open）

> 当前每个 DB 函数都 `Connection::open(&path)?`，离开作用域关闭。
> SQLite 页面缓存在连接关闭后丢失，跨函数无法共享。

- [x] **2026-06-02** 引入 `r2d2` + `r2d2_sqlite 0.27`，Tauri 中作为 managed state
  - `init_pool(app)` → `Pool<SqliteConnectionManager>`, max_size=4
  - ~30 个 db 函数 `Connection::open(&db_path(app))` → `get_conn(app)` 从池获取
  - `_path` 变体函数保持不变（CLI 测试无 AppHandle）
  - 位置：`src-tauri/src/db/mod.rs`

#### P2 — pHash 去重 O(n²) 优化

> `media_find_similar` 加载所有图片的 pHash → 双循环汉明距离比较。
> 5000 张图 = 1250 万次 u64 比较。点击"查找重复"会长时间卡住。

- [ ] 多索引预过滤（按文件大小/宽高快速排除不可能相似的图片对）
  - 位置：`src-tauri/src/db/mod.rs`，`media_find_similar` 约第 1805 行
- [ ] 可选：SIMD 加速汉明距离（`std::simd` 或 `wide` crate）

#### P2 — `resolve_thumb_paths` 消除 per-item stat()

> 每次列表查询后，对所有结果做 2× `path.exists()` 检查。
> 5000 张 = 10000 次 stat 系统调用。

- [x] 方案 B：`resolve_thumb_paths` 直接设预期路径不做 stat()，前端 `useThumbnail` 有重试兜底
  - 位置：`src-tauri/src/db/mod.rs` 约第 604 行

#### 导入管道优化（2026-05-31 审计）

> 当前导入流程：文件被打开 5 次，图片被解码 3 次，100% 串行处理。
> 典型 4MB JPEG 单文件耗时 ~200-350ms，500 张图 ≈ 2-3 分钟。

**当前流程（单文件）**：
```
文件 → SHA256(读全文件) → DB查重(全表扫描) → fs::copy(再读全文件)
     → image::open(解码) 取尺寸
     → 再次打开文件 读EXIF
     → image::open(再次解码) 算pHash
     → DB INSERT(新连接+自动提交)
     → spawn_blocking → image::open(第三次解码) → 256+512缩略图
     → spawn_blocking → 入AI队列
```

- [x] **P0 — SHA256 + copy 合并 I/O**（一次读取完成两件事）
  - 当前：`compute_sha256` 读全文件(~50ms)，然后 `fs::copy` 再读全文件(~30ms)，合计 ~80ms 重复 I/O
  - 方案：用 wrapper reader 边读边 hash 边写入目标，一次读取完成 SHA256 + copy
  - 位置：`src-tauri/src/media/import.rs`，`import_single_file` 函数

- [x] **P0 — 共享一次图片解码**（`image::open` ×3 → ×1）
  - 当前：`read_image_info`(30-80ms) + `compute_phash`(80-150ms) + `generate_thumbnails`(100-200ms) 各自独立解码
  - 合计 ~210-430ms 都在做 JPEG 解压缩
  - 方案：解码一次为 `DynamicImage`，clone 引用（Arc，不复制像素）给各步骤共享
  - 位置：`src-tauri/src/media/import.rs:138-192` + `src-tauri/src/media/thumbnail.rs:7-33` + `src-tauri/src/media/phash.rs:5-49`

- [x] **P0 — 并行处理**（3-4 路并发导入）
  - 当前：`for` 循环完全串行，文件 N+1 等文件 N 完成才启动
  - 方案：用 `tokio::task::spawn_blocking` 或 `rayon` 同时处理 3-4 个文件，I/O 和 CPU 交替利用
  - 位置：`src-tauri/src/media/import.rs:46-56`，`import_files` 函数

- [ ] **P1 — pHash 计算优化**
  - 32×32→8×8 中间 resize 用 `Nearest` 替代 `Lanczos3`（8×8 最终尺寸下质量差异无意义，速度提升 3-5x）
  - 可选：用 `rustdct` crate 替换纯 Rust 浮点 DCT（基于 FFT，O(n log n) vs O(n²)）
  - 位置：`src-tauri/src/media/phash.rs:8-10`

- [x] **P1 — 导入批次内 DB 事务**（100 次 fsync → 1 次）
  - 当前：每个文件独立的 `Connection::open` + `INSERT` + 自动提交，100 张 = 100 次 fsync
  - 方案：批次内共享连接 + `BEGIN TRANSACTION` / `COMMIT`
  - 注意：与上述 P1 "批量操作事务包装" 同根因，可一并解决
  - 位置：`src-tauri/src/media/import.rs:46-56` + `src-tauri/src/db/mod.rs:552-575`

- [x] **P2 — EXIF 复用缓冲区**（减少一次文件打开）
  - 当前：`read_exif_timestamps` 独立 `fs::File::open`，但 JPEG EXIF 在文件头部(APP1 marker)
  - 方案：从 SHA256/copy 阶段读到的首 64KB 缓冲区中解析 EXIF，不再单独打开文件
  - 位置：`src-tauri/src/media/import.rs:238-260`

**预估收益**：

| 优化项 | 单文件节省 | 100 张总收益 |
|--------|-----------|-------------|
| SHA256 + copy 合并 I/O | ~50ms | ~5s |
| 共享一次图片解码 | ~150ms | ~15s |
| 3-4 路并发处理 | — | **3-4x 总时间** |
| DB 事务批处理 | ~10ms × N | ~1s |
| EXIF 复用缓冲区 | ~5ms | ~0.5s |
| **全部合计** | **~200ms × 并发** | **500 张 < 30 秒** |

---

#### 磁盘占用优化（2026-05-31 审计）

> 5000 张 12MP JPEG 总计占用 ~20.6GB，其中 ~700MB-1.7GB 是浪费或可压缩的。

**当前存储结构**：
```
%APPDATA%/com.bronze107.medix/
├── library/         原图副本     ~20GB    ← 每张 ~4MB
├── thumbnails/
│   ├── *_256.jpg    网格/详情用   ~75MB   ← Q85 JPEG, ~15KB/张
│   └── *_512.jpg    从未使用！    ~200MB  ← Q85 JPEG, ~40KB/张
├── variants/        衍生版本     ~?GB    ← 取决于用户生成量
├── medix.db         SQLite      ~32MB   ← embeddings 占大头
└── %TEMP%/          推理临时文件  ~0.5-1GB ← 从未清理！
    └── medix_infer_*.jpg × N
```

- [x] **P0 — 去掉 thumb_512**（纯浪费 ~200MB）
  - `thumb_512` 只在类型定义中出现（`src/types/media.ts:17`、`src/lib/tauri.ts:60`），**零前端引用**
  - 但后端仍在生成：`generate_thumbnails` 输出两份、`resolve_thumb_paths` 对两种尺寸各做 `stat()`
  - 方案：`THUMB_SIZES` 只保留 256，删除 512 生成逻辑；已有 `_512.jpg` 文件需迁移清理
  - 位置：`src-tauri/src/media/thumbnail.rs:5` + `src-tauri/src/db/mod.rs:577-591` + `src/types/media.ts:17`

- [x] **P0 — 推理临时文件清理**（泄漏 ~500MB-1GB）
  - `process_generate_caption` 写入 `%TEMP%/medix_infer_{id}.jpg`，完成后**从未删除**
  - AI 处理 5000 张图 = 5000 个临时文件永久积累
  - 方案：`process_generate_caption` 函数末尾添加 `let _ = tokio::fs::remove_file(&tmp).await;`
  - 位置：`src-tauri/src/ai/mod.rs:144` + 约第 249 行（函数返回前）
  - 另需：启动时扫描并清理残留的 `medix_infer_*.jpg`

- [ ] **🐢 P1 — 缩略图格式 JPEG → WebP**（节省 ~35MB / 5000 张）
  - 256px JPEG Q85 ~15KB → WebP 同质量 ~8KB（小 40-50%）
  - WebView2 完全支持 WebP
  - **阻塞**：`image` v0.25 的 `webp` feature 仅支持解码。有损编码需要 `webp` crate（包装 libwebp C 库），纯 Rust 的 `image-webp` 也不支持有损编码。引入 C 库依赖会增加 Windows 交叉编译复杂度，且节省空间有限（5000 张仅省 ~35MB），优先级下调。

- [ ] **P1 — Embedding 存储 f32 → f16**（节省 ~15MB / 5000 张）
  - 当前：`f32::to_le_bytes()` 每维 4 字节，768 维 × 2 向量 = 6144 字节/张
  - 改为 f16（`half` crate 的 `f16::to_le_bytes()`）：每维 2 字节 → 3072 字节/张
  - 余弦相似度对 f16 精度下降不敏感，实际搜索质量几乎无影响
  - 位置：`src-tauri/src/db/mod.rs:1452`（写入）+ `db/mod.rs:1478/1553`（读取）
  - 迁移：新 embedding 用 f16 存储，读取时兼容旧 f32 数据

- [ ] **P2 — caption/tags embedding 去重**（节省 ~7.5MB / 5000 张）
  - 当前同一张图的 caption 和 tags 存了两份**完全相同的向量 BLOB**
  - 方案：改为 `content_type = "all"`，存一次；搜索时只查一行
  - 位置：`src-tauri/src/ai/mod.rs:232-243`

- [ ] **P3 — 可选的原始文件无损压缩**（节省 1-3GB / 5000 张）
  - JPEG 可通过优化 Huffman 表、去除无关元数据做到无损压缩（通常 5-15%）
  - 需设计为**可选的一键优化功能**，带预览统计，不自动执行
  - 注意：这会修改用户原始文件

**预估收益**：

| 优化 | 节省空间（5000 张） | 改动量 | 风险 |
|------|-------------------|--------|------|
| 去掉 thumb_512 | ~200MB | 小 | 无（前端零引用） |
| 清理推理临时文件 | ~500MB-1GB | 极小 | 无 |
| WebP 缩略图 | ~35MB | 中 | 需测试 WebView2 兼容 |
| f16 embedding | ~15MB | 中 | 需迁移旧数据 |
| embedding 去重 | ~7.5MB | 小 | 无 |
| 原始无损压缩 | 1-3GB | 中 | 需用户确认 |
| **合计** | **~1.8-4.3GB** | | |

---

#### 缩略图系统优化（2026-05-31 审计）

> 缩略图涉及 5 个消费端（Gallery/TableView/Lightbox/DetailPanel/VariantDropdown），
> 当前每张缩略图单独走 IPC，两个独立缓存在 Grid/Table 之间不共享。

**当前消费点**：

| 组件 | 调用 `media_thumbnail` | 缓存来源 |
|------|----------------------|---------|
| Gallery `ThumbnailCard` | ~200 张可见 | `@/hooks/useThumbnail` 的全局 Map |
| TableView `useThumbnail` | ~50 行可见 | **自己的重复 Map**（`TableView.tsx:38`） |
| Lightbox `FilmstripThumb` | ~7 张胶片条 | 共享 Gallery 的 Map |
| Lightbox `VariantThumb` | 每版本一张 | 共享 Gallery 的 Map |
| DetailPanel `MenuThumb` | 原图 + N 版本 | 共享 Gallery 的 Map；变体绕过缩略图，直接加载原图 |

- [x] **P0 — 统一缓存，消除双份 Map**
  - 当前：`@/hooks/useThumbnail.ts:5` 和 `TableView.tsx:38` 各自维护独立的 `Map<string, string>`
  - TableView 版本还没有重试逻辑，功能不一致
  - 切换网格↔表格视图，相同图片的缩略图全部重新 IPC
  - 方案：删除 TableView 内的重复 hook，统一 import `@/hooks/useThumbnail`
  - 位置：`src/components/TableView/TableView.tsx:38-60`

- [x] **P0 — 批量缩略图 IPC**（200+ 次 → 1 次）
  - 当前每个 `<ThumbnailCard>` 独立 `invoke("media_thumbnail", { id })` → 200 次 IPC 往返
  - 且每次 `media_thumbnail` 内部都做 `SELECT display_variant_id FROM media WHERE id = ?` → 200 次 DB 查询
  - 方案：新增 `media_thumbnail_batch(ids: Vec<String>)` → 返回 `Vec<{id, path}>`，一次 IPC + 一条 `WHERE id IN (...)` SQL
  - 前端 `useThumbnail` hook 改为批量预加载模式：组件 mount 时注册 ID，父组件统一调用 batch 命令
  - 位置：`src-tauri/src/commands/thumbnail.rs` + `src/hooks/useThumbnail.ts`

- [x] **P1 — 重试策略：固定间隔 → 指数退避**
  - 当前：`maxRetries = 15`，固定 2 秒间隔 = 最多 30 秒无效 IPC
  - 缩略图在导入时同步生成，3 次重试后仍缺失 = 永久缺失
  - 方案：上限 3 次，指数退避（1s → 2s → 4s = 总共 7 秒），避免突发
  - 位置：`src/hooks/useThumbnail.ts:20-21`

- [x] **P1 — 变体缩略图应使用缩略图，非原图**
  - `MenuThumb` 对 variant 用 `convertFileSrc(v.file_path)` 直接加载完整原图
  - 50 个变体 36×36px CSS 框里加载数 MB 大图 → 浏览器全解码到内存 → 数百 MB 浪费
  - 方案：variant 导入/生成时也为变体文件生成 256px 缩略图；`media_thumbnail` 支持传 variant id
  - 位置：`src/components/DetailPanel/DetailPanel.tsx:172-174` + `src-tauri/src/commands/thumbnail.rs:4` + `src-tauri/src/media/thumbnail.rs`

- [x] **P2 — `<img decoding="async">`** 
  - Gallery 的 `<img>` 有 `loading="lazy"` 但无 `decoding="async"`
  - 缺少时图片解码在主线程同步执行，200 张缩略图解码会短暂阻塞 UI
  - 方案：所有缩略图 `<img>` 加 `decoding="async"`
  - 位置：`src/components/Gallery/Gallery.tsx:308-317` + Lightbox、DetailPanel 的 `<img>`

- [ ] **P2 — 低质量占位图 (LQIP)**
  - 当前缩略图缺失时只显示灰色背景 + 尺寸文字，加载中无明显反馈
  - 方案：导入时生成 ~20px 内联 base64 缩略图存 DB（<1KB），作为模糊占位图实现渐进加载
  - 位置：`src-tauri/src/media/thumbnail.rs` + 前端 `<img>` 的 `background-image`

- [x] **P3 — 缓存 LRU 淘汰**
  - 当前 cache Map 无限增长，浏览数万张图后累积大量 URL 字符串
  - 内存影响小（~100 字节/条），但干净起见可加 LRU 限制（如 2000 条）
  - 位置：`src/hooks/useThumbnail.ts:5`

- [ ] **P3 — 按场景用不同缩略图尺寸**
  - 网格用 256px，但变体下拉框、胶片条只用 36-64px CSS
  - 可生成 64px 微缩略图给这些场景用，进一步减少内存和加载时间
  - 位置：`src-tauri/src/media/thumbnail.rs:5`

**预估收益**：

| 优化 | 效果 |
|------|------|
| 统一缓存 | 切换视图零 IPC，删除 20 行重复代码 |
| 批量 IPC | Gallery 首屏 200→1 次 IPC + 200→1 次 DB 查询 |
| 重试改退避 | 无效 IPC 流量减少 80%+ |
| 变体用缩略图 | 50 变体下拉框：数百 MB → 几 MB 内存 |
| `decoding="async"` | 主线程无阻塞 |
| LQIP | 慢速磁盘下加载体验明显提升 |

---

#### ✅ P3 — FTS5 全文搜索

> **2026-06-02 已实现**：migration 0017_fts5 + `fts_sync` 增量同步 + `fts_search` BM25 查询 + `fts_rebuild_all` 首次全量回填。与语义搜索结果合并（OR），设置页可独立开关，默认开启。

---

### 已完成（性能相关）

**2026-05-31 性能审计后实施（4 轮，11 commits）**：

- [x] **DB 索引**: `sha256`（导入查重）+ `deleted_at`（所有媒体查询）+ `embeddings.model`（语义搜索）
- [x] **媒体列表分页**: SQL `LIMIT/OFFSET` + 前端窗口化加载（500 条/页）
- [x] **去掉 thumb_512**: 零前端引用，节省 ~200MB/5000 张，缩略图生成快一半
- [x] **推理临时文件清理**: AI 标注后删除 `medix_infer_*.jpg` + 启动时扫描残留
- [x] **统一缩略图缓存**: TableView 改用共享 `useThumbnail` hook，消除重复 Map + 重复 IPC
- [x] **缩略图批量 IPC**: `media_thumbnail_batch` — Gallery/TableView 首屏 200→1 次 IPC
- [x] **批量操作事务包装**: `media_tag_add_batch` / `collection_add_batch` / `caption_create_batch` 全部 `BEGIN/COMMIT`
- [x] **缩略图重试退避**: 15 次固定 2s → 3 次指数退避（1s/2s/4s）
- [x] **SHA256 + copy 合并 I/O**: `HashingReader` 边读边 hash 边写，一次 I/O 完成两件事
- [x] **共享一次图片解码**: `image::open` ×3 → ×1，pHash/thumbnails 直接收 `&DynamicImage`
- [x] **导入 4 路并行**: `std::thread::scope` 分块并发，I/O/CPU 交替
- [x] **EXIF 复用缓冲区**: 从 copy 首 64KB 缓冲解析 EXIF，不再单独打开文件
- [x] **全局 `decoding="async"`**: Gallery/TableView/Lightbox/DetailPanel/CollectionsPage 所有 `<img>`

**2026-06-02 补充优化**：

- [x] **变体缩略图**: 变体创建时生成 256px 缩略图，`media_thumbnail` 支持变体 ID 懒生成，`MenuThumb` 缩略图优先于原图
- [x] **消除 `resolve_thumb_paths` stat()**: 直接设预期路径，前端 `useThumbnail` 有重试兜底
- [x] **缩略图缓存 LRU 淘汰**: Map 上限 2000 条，get/set 时维护访问顺序
- [x] **LQIP 低质量占位图**: 导入时生成 20px base64 JPEG（~300B），Gallery/TableView 模糊背景占位
- [x] **DB 连接池 (r2d2)**: `r2d2_sqlite 0.27` + `rusqlite 0.34`，max_size=4，~30 个 db 函数改用 `get_conn(app)`
- [x] **FTS5 全文搜索**: migration 0017，`media_fts` 虚拟表 unicode61 分词，caption/tag CRUD 自动增量同步，BM25 排序，与语义搜索结果合并
- [x] **搜索设置重构**: 语义搜索 / FTS5 独立开关 + 阈值滑块，区域改名"搜索"

**此前已完成**：

- [x] 大型网格使用虚拟滚动 + 回收 DOM（@tanstack/react-virtual）— Grid、Table 两种视图
- [x] AI 标注管道优化 — HTTP client 复用 (`LazyLock<reqwest::Client>`) + tag 缓存 + embedding 合并为单次调用 + VLM 输入默认缩放 768px
- [x] llama-server `--parallel 2` 启用并行推理

---

### 数据安全

- [ ] 定期自动备份数据库
- [x] 导入时 SHA256 精确去重（跳过重复文件）
- [x] pHash 感知哈希检测视觉相似图片（"查找重复"按钮）
- [x] 回收站机制（软删除）
- [x] 永久删除（级联清理文件 + 数据库记录）
- [x] 批量删除

### 打包发布

- [ ] Tauri 代码签名配置
- [ ] 自动更新 (Tauri updater)
- [ ] Windows Installer (MSI + NSIS)

### 验证标准
- [ ] 图库 10000 张图片，冷启动到可浏览 < 3 秒
- [ ] 连续导入 1000 张图片，内存增长 < 200MB，不泄露
- [x] 重复导入同一张图片，提示"已存在"而不是重复存储
- [x] 删除图片进入回收站，可恢复
- [x] 永久删除后文件从磁盘清除
- [x] 批量选中后一键删除
- [ ] 卸载应用后重新安装，数据库备份可恢复

---

## Phase 10: 集合系统 (Collections)

**目标**: 图片按集合分组管理，支持置顶、快速访问、批量操作

### 任务
1. [x] 数据库：`collections` + `collection_items` 表 + migration 0012
   - [x] `pinned_at` 字段支持置顶
2. [x] 后端命令（14 个）
   - [x] CRUD：`collection_list/get/create/delete/rename`
   - [x] 置顶：`collection_pin/unpin`
   - [x] 成员：`collection_add_item/add_batch/remove_item/get_item_ids`
   - [x] 查询：`media_list_by_collection`
3. [x] 侧边栏集成
   - [x] 置顶集合列表（最多 5 个）+ "全部集合" 入口
   - [x] 集合/标签/设置之间用分隔线区隔
4. [x] 集合管理页 (`/collections`)
   - [x] 卡片网格：搜索、排序、右键菜单、新建
5. [x] 集合详情页 (`/collections/:id`)
   - [x] 复用 AllMedia，自动过滤集合成员
   - [x] 集合内导入图片自动加入该集合
6. [x] 右键菜单 + 批量操作
   - [x] "添加到集合" / "从集合移除"
   - [x] 批量选择 → "添加到集合"

### 验证标准
- [x] 新建集合 → 管理页显示卡片 → 侧边栏"全部集合"可见
- [x] 置顶集合 → 侧边栏即时显示（最多 5 个）
- [x] 右键菜单添加图片到集合 → 集合详情页显示成员 → 边栏计数更新
- [x] 集合视图内拖入导入 → 图片自动归入该集合
- [x] 删除集合 → 图片保留，仅移除关联

---