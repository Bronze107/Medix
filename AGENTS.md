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
│   ├── hooks/              # React hooks
│   ├── stores/             # Zustand stores
│   ├── types/              # 全局 TypeScript 类型
│   └── lib/                # 工具函数
├── src-tauri/              # Tauri/Rust 后端
│   ├── src/
│   │   ├── commands/       # Tauri IPC 命令 (前端可调用的 Rust 函数)
│   │   ├── db/             # 数据库: schema, migrations, queries
│   │   ├── media/          # 媒体处理: 导入, 缩略图, 元数据提取
│   │   ├── ai/             # AI 推理
│   │   │   ├── mod.rs      # AiQueue 异步队列 + 任务处理
│   │   │   ├── llamacpp.rs # OpenAI 兼容 HTTP 客户端
│   │   │   └── server.rs   # llama-server 子进程生命周期管理
│   │   ├── models/         # GGUF 模型文件扫描 + 二进制/mmproj 自动检测
│   │   ├── settings/       # 设置键定义 + 默认值 getter
│   │   ├── variants/       # 变体生成: 格式转换, 裁剪, 压缩
│   │   ├── captions/       # Caption 结构体定义
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
| 标签 | `tag_` | `tag_list`, `tag_create`, `media_tag_add` |
| 变体 | `variant_` | `variant_generate`, `variant_list`, `variant_delete` |
| 描述 | `caption_` | `caption_create`, `caption_list`, `caption_update` |
| AI 服务 | `llama_server_` | `llama_server_start`, `llama_server_stop`, `llama_server_status` |
| AI 模型 | `model_list`, `auto_detect`, `embedding_info` | (无统一前缀) |
| 设置 | `settings_` | `settings_get`, `settings_set`, `settings_get_all` |
| 搜索 | `media_search` | (含 `tag:` 语法，语义搜索待 Phase 6) |
| 系统 | `greet` | (测试用途) |

## 关键约束

- **本地优先**: 所有数据处理本地完成，不上传云端（llama-server 纯本地推理）
- **导入时自动标注**: 图片导入后自动触发 AI caption + tag + embedding 生成
- **延迟生成**: 变体采用按需生成 + 缓存策略
- **内存安全**: 大图处理必须分块/流式，单图内存占用不超过 200MB
- **向后兼容**: 数据库 schema 变更需保留迁移路径，不丢失用户数据

## 测试策略

| 层级 | 类型 | 工具 |
|------|------|------|
| Rust 核心 | 单元测试 | `cargo test` |
| Rust IPC | 集成测试 | `cargo test` + mock |
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
3. 在 `src-tauri/src/` 对应的业务模块添加 Rust 结构体（如 `captions/mod.rs`）
4. `cargo check` 验证编译

### 添加 AI 模型

1. 下载 `.gguf` 文件到 `%APPDATA%/com.bronze107.medix/models/`
2. VLM 模型需同时下载 `mmproj` 文件（视觉投影器）
3. 重启 Medix → 设置页自动检测并出现在下拉列表中
4. llama-server 启动参数：`-m model.gguf --mmproj mmproj.gguf --embeddings --pooling mean`
