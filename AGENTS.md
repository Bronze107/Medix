# Medix - Agent 协作规范

> 本文件面向 AI 助手和开发工具，描述项目约定和工作流。

## 项目概况

- **名称**: Medix
- **类型**: 桌面应用 (Tauri v2 + React + Rust)
- **平台**: 主要支持 Windows 11，后续扩展 macOS/Linux
- **目的**: 本地图片/视频媒体数据集管理与标注软件

## 技术栈

| 层级 | 技术 | 版本约束 |
|------|------|----------|
| 桌面框架 | Tauri v2 | latest stable |
| 前端 | React 19 + TypeScript + Tailwind CSS | React ^19.0 |
| 前端状态 | Zustand | latest |
| 前端路由 | React Router v7 | latest |
| 后端 | Rust | edition 2021 |
| 数据库 | SQLite via `rusqlite` | latest |
| 图像处理 | `image` crate + `kamadak-exif` | latest |
| 视频处理 | ffmpeg/ffprobe (sidecar 捆绑) | latest stable |
| AI 推理 | llama.cpp `llama-server` (OpenAI 兼容 HTTP API) | latest |
| 搜索 | SQLite FTS5 | built-in |
| 浏览器插件 | Chrome Extension Manifest V3 | - |

## 目录约定

```
Medix/
├── src/                    # 前端代码 (React + TS)
│   ├── components/         # UI 组件，按功能分子目录
│   │   ├── AllMedia/       # 媒体浏览主视图 (网格/表格 + 搜索/排序/分组)
│   │   ├── CollectionsPage/ # 集合管理页
│   │   ├── DetailPanel/    # 右侧详情面板 (详情/描述/版本)
│   │   ├── DropZone/       # 拖拽导入区域
│   │   ├── ExportDialog/   # 导出向导对话框
│   │   ├── Gallery/        # 网格视图 (虚拟滚动)
│   │   ├── Layout/         # 全局布局 (侧边栏 + 主内容区)
│   │   ├── Lightbox/       # 原图查看 + 版本对比
│   │   ├── SearchBar/      # 搜索栏 (语法高亮 pill)
│   │   ├── Settings/       # 设置页面
│   │   ├── TableView/      # 列表视图 (虚拟滚动)
│   │   ├── Tags/           # 标签管理页
│   │   ├── Toast/          # Toast 通知组件
│   │   └── Trash/          # 回收站页面
│   ├── hooks/              # React hooks
│   ├── stores/             # Zustand stores
│   ├── types/              # 全局 TypeScript 类型
│   └── lib/                # 工具函数
├── src-tauri/              # Tauri/Rust 后端
│   ├── src/
│   │   ├── commands/       # Tauri IPC 命令 (前端可调用的 Rust 函数)
│   │   ├── db/             # 数据库: schema, migrations, queries
│   │   ├── media/          # 媒体处理: 图片/视频导入, 缩略图, 元数据提取, pHash 去重
│   │   │   ├── import.rs         # 图片导入管线
│   │   │   ├── video_import.rs   # 视频导入管线 (ffprobe 验证 + ffmpeg 缩略图)
│   │   │   ├── video_metadata.rs # ffprobe 元数据提取 + 多帧抽取
│   │   │   ├── video_thumbnail.rs # ffmpeg 视频抽帧缩略图
│   │   │   ├── thumbnail.rs      # 图片缩略图 + LQIP
│   │   │   └── phash.rs          # 图片感知哈希去重
│   │   ├── ai/             # AI 推理 (图片 + 视频)
│   │   │   ├── mod.rs      # AiQueue 异步队列 + 图片/视频标注任务处理
│   │   │   ├── llamacpp.rs # OpenAI 兼容 HTTP 客户端
│   │   │   ├── server.rs   # llama-server / embedding-server 子进程管理
│   │   │   └── imagine/    # AI 图像生成 (Image API)
│   │   ├── models/         # GGUF 模型文件扫描 + 二进制/mmproj 自动检测
│   │   ├── settings/       # 设置键定义 + 默认值 getter
│   │   ├── variants/       # 版本控制: 生成 + 外部导入
│   │   ├── captions/       # Caption 结构体定义
│   │   ├── search/         # 搜索引擎: 语义搜索 + 结构化过滤
│   │   ├── export/         # 数据集导出: 目录/ZIP, caption 选择
│   │   ├── server/         # 本地 HTTP 服务: 浏览器插件通信
│   │   └── tag/            # Tag 结构体定义
│   ├── benches/         # 性能基准测试 (criterion)
│   │   ├── phash_bench.rs
│   │   ├── parser_bench.rs
│   │   ├── import_bench.rs
│   │   └── search_bench.rs
│   └── Cargo.toml
├── tests/                   # CLI 后端回归测试脚本
│   ├── _helpers.sh          # 共享测试函数库
│   ├── search.sh
│   ├── integrity.sh
│   ├── operations.sh
│   ├── tags-collections.sh
│   ├── cascade.sh
│   └── variants-browse.sh
├── extension/              # Chrome/Firefox 浏览器插件
│   ├── manifest.json
│   ├── background.js
│   ├── content.js
│   └── popup.html
├── docs/                   # 设计文档和决策记录
│   └── decisions/          # ADR (Architecture Decision Records)
├── PLAN.md                 # 项目阶段计划 (9 phases)
└── AGENTS.md               # 本文件
```

## 编码规范

### Rust

- 使用 `rustfmt` 默认配置，提交前必须格式化
- 所有 `pub` 函数必须有文档注释
- Tauri command 函数命名：`snake_case`，返回 `Result<T, String>` 或自定义错误类型
- 数据库操作集中在 `db/` 模块，不直接在各业务模块写 SQL
- 异步操作使用 `tokio`，图像处理等 CPU 密集型任务使用 `tokio::task::spawn_blocking`
- 错误处理优先使用 `thiserror`，避免裸 `unwrap()`

### TypeScript / React

- 严格模式 (`strict: true`)，不允许 `any`
- 组件文件命名：`PascalCase.tsx`
- Hook 文件命名：`useCamelCase.ts`
- 工具函数：`camelCase.ts`
- 类型定义：优先使用 `interface`，组件 Props 命名为 `{ComponentName}Props`
- 不使用默认导出 (`export default`)，使用命名导出
- Tailwind 类名顺序：布局 > 尺寸 > 间距 > 外观 > 交互

### 数据库

- Schema 变更在 `db/mod.rs` 的 `run_migrations()` 中追加，使用 `INSERT OR IGNORE INTO _migrations` 追踪
- 已有列的条件添加使用 `pragma_table_info` 检查（避免 ALTER TABLE 重复报错）
- 主键使用 ULID (TEXT)，避免自增 ID
- 时间戳统一使用 `DATETIME` 存储 ISO 8601 字符串
- Embedding 向量以 `f32::to_le_bytes()` 存为 BLOB

## 开发工作流

1. **阶段开发**: 按 `PLAN.md` 的 Phase 推进，每阶段完成时更新 `PLAN.md` 勾选状态
2. **提交规范**: 遵循 Conventional Commits
   - `feat:` 新功能
   - `fix:` 修复
   - `refactor:` 重构
   - `perf:` 性能优化
   - `docs:` 文档
   - `test:` 测试
   - `chore:` 构建/工具
3. **代码审查**: 关键模块 (db, ai, export) 的变更必须有 Rust 单元测试；性能敏感路径建议追加 criterion benchmark
4. **ADR**: 架构决策记录在 `docs/decisions/YYYY-MM-DD-title.md`
5. **回归测试**: 后端功能变更必须追加 CLI 回归用例到 `tests/*.sh`，提交前全量通过；前端组件变更追加 Vitest 用例到 `*.test.tsx`

## Tauri Command 命名约定

前端调用 Rust 函数的命名模式：

| 模块 | 前缀 | 示例 |
|------|------|------|
| 媒体 | `media_` | `media_import`, `media_list`, `media_search` |
| 删除/回收站 | `media_` | `media_soft_delete`, `media_recover`, `media_permanent_delete`, `media_empty_trash`, `media_list_trash` |
| 去重 | `media_` | `media_find_duplicates` |
| 标签 | `tag_` / `media_tag_` | `tag_list`, `tag_create`, `media_tag_add`, `media_tag_add_batch`, `media_tag_remove`, `media_tag_remove_batch`, `media_tags_intersect` |
| 版本 | `variant_` | `variant_generate`, `variant_import`, `variant_list`, `variant_delete`, `variant_presets` |
| 描述 | `caption_` | `caption_create`, `caption_create_batch`, `caption_list`, `caption_update`, `caption_delete` |
| AI 服务 | `llama_server_` | `llama_server_start`, `llama_server_stop`, `llama_server_status` |
| AI 标注 | `media_ai_annotate`, `ai_pending_count` | 手动触发 AI 标注（自动区分图片/视频）+ 查询排队数 |
| AI 模型 | `model_list`, `auto_detect`, `embedding_info` | (无统一前缀) |
| 设置 | `settings_` | `settings_get`, `settings_set`, `settings_get_all`（含视频: `video_large_file_warning_mb`, `video_ai_enabled`, `video_ai_frame_count`, `video_ai_multi_frame`；AI 语言: `ai_language`）|
| 筛选器 | `saved_filters_` | `saved_filters_list`, `saved_filters_save`, `saved_filters_delete` |
| 导出 | `export_dataset`, `import_zip` | (无统一前缀) |
| 文件路径 | `media_get_paths`, `media_thumbnail` | (无统一前缀) |
| 集合 | `collection_` | `collection_list`, `collection_create`, `collection_delete`, `collection_pin`, `collection_add_item`, `collection_add_batch`, `media_list_by_collection` |
| 系统 | `greet` | (测试用途) |

## 关键约束

- **本地优先**: 所有数据处理本地完成，不上传云端（llama-server 纯本地推理）
- **媒体类型**: `media` 表通过 `media_type` 字段区分 `"image"` / `"video"`，旧数据默认为 `image`
- **视频支持**: 视频通过 ffmpeg/ffprobe sidecar 导入，生成缩略图，通过 `asset://` 协议播放（Tauri v2.11 已验证 Range 请求支持 seek）
- **导入时自动标注**: 图片导入后自动触发 AI caption + tag + embedding 生成；视频导入后若 `video_ai_enabled` 开启则自动多帧 AI 标注（支持逐帧独立推理或 `video_ai_multi_frame` 多图合并推理）。通过 `ai_language` 设置可选择标注语言：English / 中文 / 双语（英文 + 中文各一条独立 caption，source 分别为 `ai_en` / `ai_zh`）
- **版本控制**: 同一原图/视频支持多个衍生版本（内部生成 + 外部导入），支持图片/视频混合 variant
- **集合**: 图片/视频可按集合分组管理，支持置顶常用集合、导入自动归集、集合内搜索
- **视图分组**: 网格和列表视图可按日期分组显示，带分组标题和计数
- **导入进度**: 批量导入时前端实时显示进度条（Tauri event `import-progress`）
- **文件服务**: 图片和视频通过 `asset://` 协议直出（`convertFileSrc`），不经过 base64 编解码；视频支持 Range 请求实现 seek
- **向后兼容**: 数据库 schema 变更采用 `pragma_table_info` 条件检查，不丢失用户数据

## 测试策略

### 总览

| 层级 | 类型 | 工具 | 当前规模 |
|------|------|------|----------|
| 后端 CLI | 回归测试 | `medix-cli` + `tests/*.sh` | 6 脚本, 120 断言 |
| Rust 核心 | 单元测试 | `cargo test` | 43 tests (parser + db + search + media + export) |
| Rust 核心 | 性能基准 | `cargo bench` (criterion) | 4 套件 (phash, parser, import, search) |
| 前端组件 | 单元测试 | Vitest + @testing-library/react | 26 tests (SearchBar, ConfirmDialog, appStore) |
| Rust 核心 | Fuzz 测试 | proptest | 4 property tests (parser 随机输入) |

### CLI 回归测试（优先级最高）

所有脚本统一通过 `tests/_helpers.sh` 共享工具函数。写入型脚本使用 **隔离临时数据库**，`trap EXIT` 自动清理，零生产数据风险。

```
cd src-tauri && bash ../tests/<name>.sh
```

| 脚本 | 用例 | DB 模式 | 覆盖范围 |
|------|------|---------|----------|
| `tests/search.sh` | 19 | 隔离 + seed 30 条 | 标签、尺寸、文件大小、media_type、混合、精确断言 |
| `tests/integrity.sh` | 26 | 生产(只读) | 6 表孤儿记录、schema 版本、视频迁移、迁移幂等 |
| `tests/operations.sh` | 26 | 隔离 | 软删除→恢复、集合、SHA256、视频导入(条件) |
| `tests/tags-collections.sh` | 15 | 隔离 | 标签 CRUD、批量标签、交集、集合置顶、集合内搜索 |
| `tests/cascade.sh` | 21 | 隔离 | FK 级联删除(5 表)、caption/variant CRUD |
| `tests/variants-browse.sh` | 13 | 隔离 | variant 浏览模式 (representative/all)、display_variant 回退 |

**开发规范**：后端功能变更必须在对应测试脚本追加用例，提交前 `bash tests/*.sh` 全量通过。

### Rust 单元测试

覆盖 5 个核心模块，使用 `tempfile` 创建隔离 SQLite 数据库：

```bash
cd src-tauri && cargo test --lib     # 43 tests
```

| 模块 | 文件 | 覆盖 |
|------|------|------|
| `search::parser` | `parser.rs` (17 tests + 4 proptest) | 查询解析 + 随机输入不 panic |
| `db` | `mod.rs` (11 tests) | schema 幂等、CRUD、FK 级联、软删除、settings |
| `search` | `search_tests.rs` (6 tests) | execute_search_path 标签/尺寸/大小/混合 |
| `media` | `media_tests.rs` (9 tests) | pHash 确定性、magic bytes、is_supported |
| `export` | `export_tests.rs` (1 test) | is_ai_source |

### 性能基准测试

基于 criterion，覆盖 3 个数据量级（100 / 1k / 10k 条记录）：

```bash
cd src-tauri && cargo bench              # 全部 (~5 min)
cargo bench --bench phash_bench          # 单个套件
cargo bench -- --save-baseline base      # 保存基准
cargo bench -- --baseline base           # 与基准对比检测回归
open target/criterion/report/index.html  # HTML 报告
```

| 套件 | 覆盖 |
|------|------|
| `phash_bench` | DCT pHash (8x8 + 512x512)、hamming_distance |
| `parser_bench` | 简单/复杂/quoted/中文查询解析 |
| `import_bench` | magic byte 检测、is_supported |
| `search_bench` | execute_search_path + find_items_with_tags (100/1k/10k) |

### 前端单元测试

```bash
npm test            # 26 tests
npm run test:watch  # 交互模式
```

| 组件 | 文件 | 测试数 | 覆盖 |
|------|------|--------|------|
| SearchBar | `SearchBar.test.tsx` | 13 | 渲染、pill 解析/删除、tag/width/height/date/size、大小写 |
| ConfirmDialog | `ConfirmDialog.test.tsx` | 8 | 渲染/隐藏、确认/取消/遮罩点击、自定义文本、danger 样式 |
| appStore | `appStore.test.ts` | 5 | 初始状态、sidebar/detail 切换、选中媒体 |

### CLI 命令速查

```bash
# 数据库管理
cargo run --bin medix-cli -- setup-db --db-path /tmp/test.db   # 初始化空库
cargo run --bin medix-cli -- seed -c 100 --with-collections --db-path /tmp/test.db

# 搜索与浏览
cargo run --bin medix-cli -- search "tag:cat width:>1920"      # 搜索
cargo run --bin medix-cli -- search -n "tag:cat"               # 仅返回计数
cargo run --bin medix-cli -- list                               # 列出全部媒体
cargo run --bin medix-cli -- list -n                            # 仅返回数量
cargo run --bin medix-cli -- list-tags                          # 列出标签
cargo run --bin medix-cli -- list-tags -n                       # 标签数
cargo run --bin medix-cli -- list-collections                   # 列出集合
cargo run --bin medix-cli -- list-variants <media_id>           # 列出版本

# 数据查询与写入
cargo run --bin medix-cli -- query "SELECT ..."                 # 只读 SQL
cargo run --bin medix-cli -- exec "INSERT/UPDATE/DELETE ..."    # 写 SQL (测试用)
cargo run --bin medix-cli -- stats                              # 统计概览

# 全局 flag
--db-path <path>    # 指定数据库路径
--json              # JSON 输出（机器可解析）
```

## 常见任务速查

### 添加新的 Tauri Command

1. 在 `src-tauri/src/commands/` 下新建或修改对应模块
2. 函数签名：`#[tauri::command] pub async fn command_name(...) -> Result<T, String>`
3. 在 `src-tauri/src/main.rs` 的 `invoke_handler` 中注册
4. 前端在 `src/lib/tauri.ts` 添加类型安全的包装函数

### 添加数据库表

1. 在 `src-tauri/src/db/mod.rs` 的 `run_migrations()` 末尾追加 `INSERT OR IGNORE` + `CREATE TABLE IF NOT EXISTS`
2. 在同一文件添加对应的 CRUD 函数，参数模式 `fn xxx(app: &AppHandle, ...)`
3. 同时在对应 `_path` 变体函数中使用 `&Path` 替代 `&AppHandle`（供 CLI 调用）
4. 在 `src-tauri/src/` 对应的业务模块添加 Rust 结构体（如 `captions/mod.rs`）
5. `cargo check` 验证编译

### 编写 CLI 后端回归测试

测试脚本通过 `source "$(dirname "$0")/_helpers.sh"` 引入共享函数库，写入型测试使用隔离临时数据库。

```
tests/<name>.sh
├── source _helpers.sh          # 共享函数库
├── setup_isolated_db <hint> [n] # 创建临时 DB，可选 seed n 条记录
├── cli()    → cargo run --bin medix-cli -- <cmd>  (自动附加 --db-path)
├── q()      → cli query "<SQL>"                    (只读查询)
├── exec_sql() → cli exec "<SQL>"                   (写操作)
├── search_count() → cli search -n "<query>"        (精确计数)
├── media_count()  → cli list -n                    (媒体总数)
├── tag_count()    → cli list-tags -n               (标签总数)
├── check()  → 断言 expected == actual
└── final_report → 打印汇总并 exit
```

测试模式：
- **隔离优先**：写入型脚本调用 `setup_isolated_db` 使用临时 DB，`trap EXIT` 自动清理
- **种子数据**：`setup_isolated_db "hint" 30` 自动调用 `seed -c 30 --with-collections` 创建 8 个标准 tag + 4 个集合
- **精确断言**：优先用 `search_count` / `media_count` / `tag_count`（依赖 CLI `--count` flag），避免 `grep`/`sed` 解析人类可读输出
- **只读测试**：integrity.sh 仍使用生产 DB（纯 SELECT），其余 5 个脚本全部隔离
- **数据清洁**：隔离模式下无需手动清理；如必须操作生产 DB，使用 `_test_` 前缀 ID 并在末尾还原

### 配置 ffmpeg Sidecar

1. 从 [ffmpeg.org](https://ffmpeg.org/download.html) 下载 Windows builds (lgpl)
2. 将 `ffmpeg.exe` 重命名为 `ffmpeg-x86_64-pc-windows-msvc.exe`，放入 `src-tauri/binaries/`
3. 将 `ffprobe.exe` 重命名为 `ffprobe-x86_64-pc-windows-msvc.exe`，放入 `src-tauri/binaries/`
4. 文件名需匹配 Tauri sidecar 目标三元组约定
5. 运行时自动从 exe 同目录或 `binaries/` 目录发现，无需手动配置 PATH
6. ffmpeg/ffprobe 用于视频导入验证、元数据提取、缩略图生成、AI 多帧抽取

### 添加视频支持功能

- **Schema**: 新增 `media_type`/`duration`/`video_codec`/`video_fps` 列到 `media` 和 `variants` 表
- **导入**: 视频文件经 `video_import.rs` 导入，复用 SHA256 去重，跳过 pHash
- **缩略图**: ffmpeg 从 10% 时间点抽帧，fallback: 1s → 50% → 第一帧
- **播放**: `convertFileSrc()` → `asset://` 协议 → `<video>` 元素，Tauri v2.11 asset 协议支持 Range 请求 (1MB/range)
- **搜索**: `media_type:image` / `media_type:video` 结构化过滤
- **AI 标注**: 视频导入后若 `video_ai_enabled=true`，自动 ffmpeg 抽取 1-8 帧（默认 3），发送给 VLM 推理。支持两种模式：① 逐帧独立推理（默认）— 每帧单独调用 VLM，最后合并 caption/tags；② `video_ai_multi_frame` 多图合并推理 — 所有帧在一次请求中发送（帧间用 "1/3" 标签区分），模型可理解帧间时序关系（需 VLM 支持多图输入，如 Qwen2-VL、InternVL2）。`media_ai_annotate` 命令自动区分图片/视频类型
- **AI 标注语言**: 通过 `ai_language` 设置选择标注语言（`en` / `zh` / `bilingual`）。图片和视频 AI 标注均支持。双语模式下调用两次 VLM（英文 + 中文），生成两条独立 caption（source: `ai_en` / `ai_zh`），标签统一使用英文 danbooru 风格。视频多帧推理的用户消息文本（帧标签、结尾指令）会随语言设置切换中英文

### 添加 AI 模型

1. 下载 `.gguf` 文件到 `%APPDATA%/com.bronze107.medix/models/`
2. VLM 模型需同时下载 `mmproj` 文件（视觉投影器）
3. 重启 Medix → 设置页自动检测并出现在下拉列表中
4. llama-server 启动参数：`-m model.gguf --mmproj mmproj.gguf --embeddings`
