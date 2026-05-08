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
| AI 推理 | ONNX Runtime (`ort`) | 2.x |
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
│   │   ├── ai/             # AI 推理: 模型加载, tag 推理
│   │   ├── variants/       # 变体生成: 格式转换, 裁剪, 压缩
│   │   ├── export/         # 导入导出: ZIP, COCO, YOLO
│   │   └── server/         # HTTP 服务 (供浏览器插件调用)
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

- Schema 变更必须写迁移脚本，按 `YYYYMMDD_HHMMSS_description.sql` 命名
- 主键使用 ULID (TEXT)，避免自增 ID
- 时间戳统一使用 `DATETIME` 存储 ISO 8601 字符串
- JSON 字段使用 SQLite JSON 扩展，Rust 侧对应 `serde_json::Value`

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
| 媒体 | `media_` | `media_import`, `media_list`, `media_delete` |
| 标签 | `tag_` | `tag_list`, `tag_add_to_media`, `tag_remove` |
| 变体 | `variant_` | `variant_generate`, `variant_list`, `variant_delete` |
| 标注 | `annotation_` | `annotation_create`, `annotation_list` |
| 搜索 | `search_` | `search_query`, `search_suggestions` |
| 导出 | `export_` | `export_dataset`, `export_progress` |
| 系统 | `sys_` | `sys_get_settings`, `sys_set_settings` |

## 关键约束

- **本地优先**: 所有数据处理本地完成，不上传云端
- **延迟生成**: 变体、AI 标签等采用按需生成 + 缓存策略
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

1. 在 `src-tauri/src/db/migrations/` 创建迁移文件
2. 在 `src-tauri/src/db/schema.rs` 更新表结构定义
3. 在 `src-tauri/src/db/queries.rs` 添加 CRUD 函数
4. 运行 `cargo test --test db` 验证

### 添加 AI 模型

1. 下载 `.onnx` 文件到 `models/` (gitignored)
2. 在 `src-tauri/src/ai/` 添加模型加载和推理代码
3. 首次运行时自动下载的机制放在 `src-tauri/src/ai/download.rs`
4. 模型文件不超过 500MB，否则提供外部下载链接
