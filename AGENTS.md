# Medix - Agent 协作规范

> 本文件面向 AI 助手和开发工具，描述项目约定和工作流。

## 项目概况

- **名称**: Medix
- **类型**: 桌面应用 (Tauri v2 + React + Rust)
- **平台**: 主要支持 Windows 11，后续扩展 macOS/Linux
- **目的**: 本地媒体数据集管理与压缩软件

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
│   │   ├── media/          # 媒体处理: 导入, 缩略图, 元数据提取, pHash 去重
│   │   ├── ai/             # AI 推理
│   │   │   ├── mod.rs      # AiQueue 异步队列 + 任务处理
│   │   │   ├── llamacpp.rs # OpenAI 兼容 HTTP 客户端
│   │   │   └── server.rs   # llama-server 子进程生命周期管理
│   │   ├── models/         # GGUF 模型文件扫描 + 二进制/mmproj 自动检测
│   │   ├── settings/       # 设置键定义 + 默认值 getter
│   │   ├── variants/       # 版本控制: 生成 + 外部导入
│   │   ├── captions/       # Caption 结构体定义
│   │   ├── search/         # 搜索引擎: 语义搜索 + 结构化过滤
│   │   ├── export/         # 数据集导出: 目录/ZIP, caption 选择
│   │   ├── server/         # 本地 HTTP 服务: 浏览器插件通信
│   │   └── tag/            # Tag 结构体定义
│   └── Cargo.toml
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
3. **代码审查**: 关键模块 (db, ai, export) 的变更必须有 Rust 单元测试
4. **ADR**: 架构决策记录在 `docs/decisions/YYYY-MM-DD-title.md`

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
| AI 标注 | `media_ai_annotate`, `ai_pending_count` | 手动触发 AI 标注 + 查询排队数 |
| AI 模型 | `model_list`, `auto_detect`, `embedding_info` | (无统一前缀) |
| 设置 | `settings_` | `settings_get`, `settings_set`, `settings_get_all` |
| 筛选器 | `saved_filters_` | `saved_filters_list`, `saved_filters_save`, `saved_filters_delete` |
| 导出 | `export_dataset`, `import_zip` | (无统一前缀) |
| 文件路径 | `media_get_paths`, `media_thumbnail` | (无统一前缀) |
| 集合 | `collection_` | `collection_list`, `collection_create`, `collection_delete`, `collection_pin`, `collection_add_item`, `collection_add_batch`, `media_list_by_collection` |
| 系统 | `greet` | (测试用途) |

## 关键约束

- **本地优先**: 所有数据处理本地完成，不上传云端（llama-server 纯本地推理）
- **导入时自动标注**: 图片导入后自动触发 AI caption + tag + embedding 生成，详情面板可手动重跑
- **版本控制**: 同一原图支持多个衍生版本（内部生成 + 外部导入），带自定义标签和来源追踪
- **集合**: 图片可按集合分组管理，支持置顶常用集合、导入自动归集、集合内搜索
- **视图分组**: 网格和列表视图可按日期分组显示，带分组标题和计数
- **导入进度**: 批量导入时前端实时显示进度条（Tauri event `import-progress`）
- **内存安全**: 图片通过 `asset://` 协议直出，不经过 base64 编解码
- **向后兼容**: 数据库 schema 变更采用 `pragma_table_info` 条件检查，不丢失用户数据

## 测试策略

### CLI 后端回归测试（优先级最高）

`medix-cli` 可直接操作生产 DB 验证后端功能，无需启动 GUI。测试脚本在 `tests/` 目录。

```
cd src-tauri && bash ../tests/<name>.sh
```

| 脚本 | 用例 | 覆盖范围 |
|------|------|----------|
| `tests/search.sh` | 16 | 标签过滤(交集/并集/不存在)、尺寸(>/</范围)、文件大小、混合查询、纯文本回归、边缘情况 |
| `tests/integrity.sh` | 17 | 6 表孤儿记录检测、计数一致性、schema 版本、活跃媒体字段完整性 |
| `tests/operations.sh` | 21 | 软删除→恢复、搜索排除已删除、集合增删成员、SHA256 去重、schema 全表存在、设置读写 |
| `tests/tags-collections.sh` | 13 | 标签 CRUD、批量标签、交集查询、集合置顶/取消置顶、集合内搜索 |
| `tests/cascade.sh` | 20 | FK 级联删除(5 表: captions/embeddings/variants/media_tags/collection_items)、caption/variant CRUD、筛选器保存/删除、pHash 数据 |

**开发规范**：任何后端功能变更（搜索语法、DB schema、CRUD 操作）必须在对应测试脚本追加用例，提交前 `bash tests/*.sh` 全量通过。

### CLI 命令速查

```bash
cargo run --bin medix-cli -- search "tag:cat"           # 搜索测试
cargo run --bin medix-cli -- query "SELECT ..."         # 只读查询
cargo run --bin medix-cli -- exec "UPDATE/DELETE ..."   # 写操作（仅测试用）
cargo run --bin medix-cli -- list                       # 列出全部媒体
cargo run --bin medix-cli -- list-tags                  # 列出全部标签
cargo run --bin medix-cli -- stats                      # 统计概览
```

### 层级策略

| 层级 | 类型 | 工具 |
|------|------|------|
| 后端 CLI | 回归测试 | `medix-cli` + `tests/*.sh` |
| Rust 核心 | 单元测试 | `cargo test` |
| 前端组件 | 视觉测试 | Storybook |
| 端到端 | 场景测试 | Playwright (Tauri 模式) |

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

CLI 测试无需启动 GUI，直接操作生产 DB（共享同一数据库路径 `%APPDATA%/com.bronze107.medix/medix.db`）。

```
tests/<name>.sh
├── cli()    → cargo run --bin medix-cli -- <command>
├── q()      → cli query "<SQL>"       (只读查询)
├── exec_sql() → cli exec "<SQL>"      (写操作，测试后必须还原)
└── check()  → 断言 expected == actual
```

测试模式：
- **查询验证**：`q "SELECT COUNT(*) FROM ..."` 获取数据，与 CLI 命令结果交叉验证
- **操作还原**：写操作前保存原始值，测试后通过 SQL 还原（如软删除→恢复）
- **数据清洁**：创建的测试记录使用 `_test_` 或 `_cli_` 前缀 ID，测试末尾清理

### 添加 AI 模型

1. 下载 `.gguf` 文件到 `%APPDATA%/com.bronze107.medix/models/`
2. VLM 模型需同时下载 `mmproj` 文件（视觉投影器）
3. 重启 Medix → 设置页自动检测并出现在下拉列表中
4. llama-server 启动参数：`-m model.gguf --mmproj mmproj.gguf --embeddings --pooling mean`
