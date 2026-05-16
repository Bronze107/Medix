# Medix 构建指南

从零开始构建和运行 Medix 的完整指南。

## 前置条件

| 工具 | 最低版本 | 说明 |
|------|----------|------|
| Rust | 1.78+ | [rustup.rs](https://rustup.rs) 安装，MSVC toolchain（Windows） |
| Node.js | 18+ | [nodejs.org](https://nodejs.org) 或 nvm |
| npm | 9+ | 随 Node.js 附带 |
| Git | 2.0+ | [git-scm.com](https://git-scm.com) |
| llama-server | 最新 | 见下方 [llama.cpp 设置](#llamacpp-设置) |

### Windows 额外要求

- **Microsoft Visual C++ Build Tools**: 安装 Visual Studio 2022，勾选"使用 C++ 的桌面开发"工作负载。或单独安装 [Build Tools for Visual Studio](https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022)。
- **WebView2**: Windows 10/11 自带。若缺失，Tauri 会提示安装。

### macOS 额外要求

```bash
xcode-select --install
```

### Linux 额外要求

```bash
# Ubuntu/Debian
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
  librsvg2-dev libjavascriptcoregtk-4.1-dev libsoup-3.0-dev

# Fedora
sudo dnf install webkit2gtk4.1-devel gtk3-devel libappindicator-gtk3-devel \
  librsvg2-devel
```

Arch 用户参考 [Tauri Linux 文档](https://v2.tauri.app/start/prerequisites/#linux)。

## 快速开始（开发模式）

```bash
# 1. 克隆仓库
git clone https://github.com/bronze107/medix.git
cd medix

# 2. 安装前端依赖
npm install

# 3. 准备 llama-server（见下方）

# 4. 启动开发环境
npm run tauri dev
```

`npm run tauri dev` 会同时启动 Vite 开发服务器（端口 1420）和 Tauri 桌面窗口。前端代码修改会自动热更新；Rust 代码修改需要重新编译（自动触发）。

## llama.cpp 设置

Medix 的 AI 自动标注功能需要 `llama-server` 和 Vision Language Model。

### 1. 下载 llama.cpp

从 [llama.cpp releases](https://github.com/ggerganov/llama.cpp/releases) 下载预编译版本：

**Windows（推荐 Vulkan 加速）**:
```
llama-bXXXX-bin-win-llama-x64-rpc-server.zip
```
解压到 `C:\llama-vulkan\`，确保 `llama-server.exe` 在解压目录中。

**macOS (Metal)**:
```
llama-bXXXX-bin-macos-arm64.zip
```

**Linux (CUDA)**:
```
llama-bXXXX-bin-ubuntu-x64-cuda12.zip
```

### 2. 下载模型

推荐 MiniCPM-V 2.6（~1GB，Q4 量化，支持视觉和文本）：

1. 在 Medix 项目根目录创建 `models/` 目录
2. 从 HuggingFace 下载模型文件和投影器：
   - `MiniCPM-V-2_6-Q4_K_M.gguf`
   - `mmproj-MiniCPM-V-2_6-f16.gguf`

```bash
mkdir models
# 使用 huggingface-cli 或浏览器下载，放入 models/ 目录
```

> `models/` 已在 `.gitignore` 中排除，不会提交到仓库。

### 3. 应用内配置

启动 Medix 后，进入 **设置** 页面：
- **二进制路径**: 选择 `llama-server.exe` 的路径（如 `C:\llama-vulkan\llama-server.exe`）
- **模型文件**: 选择 `models/` 下的 `.gguf` 文件
- **MMProj 文件**: 选择 `mmproj-` 开头的投影器文件
- 点击 **启动服务器**，状态灯变绿即就绪

导入图片后，AI 会自动生成描述和标签。

### 不使用 AI 功能

如果不需要 AI 自动标注，Medix 的其他功能（导入、浏览、标签、搜索、导出）均可正常使用，无需配置 `llama-server`。

## 生产构建

```bash
npm run tauri build
```

构建产物在 `src-tauri/target/release/bundle/`：
- **Windows**: `.msi` 安装包 和 `.exe` NSIS 安装包
- **macOS**: `.dmg` 磁盘镜像
- **Linux**: `.deb` 和 `.AppImage`

单次构建输出路径：`src-tauri/target/release/medix`（或 `medix.exe`）。

### 构建优化

`Cargo.toml` 已配置 release profile：
- `opt-level = 3` — 最高优化等级
- `lto = true` — 链接时优化（编译更慢，产物更小更快）
- `strip = true` — 去除调试符号

调试构建时如需加速，可临时注释 `lto = true`。

## 项目结构

```
Medix/
├── src/                    # React 前端源码
│   ├── components/         # UI 组件
│   │   ├── AllMedia/       # 媒体网格浏览
│   │   ├── DetailPanel/    # 图片详情面板
│   │   ├── Layout/         # 侧边栏布局
│   │   ├── SearchBar/      # 搜索栏
│   │   ├── Settings/       # 设置页面
│   │   └── Trash/          # 回收站
│   ├── lib/tauri.ts        # Tauri IPC 调用封装
│   ├── stores/             # Zustand 状态管理
│   └── types/              # TypeScript 类型定义
├── src-tauri/              # Rust 后端
│   ├── src/
│   │   ├── main.rs         # 应用入口 + Tauri 配置
│   │   ├── commands/       # Tauri IPC 命令处理器
│   │   ├── db/             # SQLite 数据库 + 迁移
│   │   ├── ai/             # AI 推理（llama-server 管理 + HTTP 客户端）
│   │   ├── media/          # 媒体处理（导入 + 缩略图 + pHash）
│   │   ├── search/         # 搜索（解析 + 语义搜索）
│   │   ├── settings/       # 设置键定义
│   │   ├── variants/       # 变体生成
│   │   ├── export/         # 数据集导出
│   │   └── server/         # 本地 HTTP 服务（浏览器插件）
│   ├── Cargo.toml
│   └── tauri.conf.json     # Tauri 配置
├── extension/              # Chrome 浏览器插件
├── models/                 # AI 模型（gitignored）
└── PLAN.md                 # 项目路线图
```

## 环境变量

| 变量 | 说明 | 默认值 |
|------|------|--------|
| `TAURI_DEV_HOST` | 开发模式 HMR 主机 IP | 无（仅 localhost） |
| `TAURI_SIGNING_PRIVATE_KEY` | 生产签名密钥 | 无 |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 签名密钥密码 | 无 |

## 常见问题

### `error: linker 'link.exe' not found` (Windows)

未安装 MSVC 工具链。安装 [Visual Studio 2022 Build Tools](https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022)，确保勾选"C++ 桌面开发"。

### `rusqlite` 编译失败

`rusqlite` 使用了 `bundled` feature，会从源码编译 SQLite。需要 C 编译器（Windows: MSVC, macOS: Xcode, Linux: gcc/clang）。

### `tauri dev` 启动后白屏

1. 确认 Vite 开发服务器正常启动（终端应显示 `http://localhost:1420`）
2. 检查 `npm run dev` 能否独立运行
3. Windows: 确保防火墙未拦截 localhost:1420

### llama-server 启动失败

1. 检查二进制路径是否正确（设置页可浏览选择）
2. Windows: 确认 `llama-server.exe` 的 Vulkan DLL 在同一目录
3. 查看终端日志 `[ai]` 前缀的错误信息
4. 尝试在终端手动运行 `llama-server` 排查启动参数问题

### 端口冲突

- Vite 开发服务器: 1420（在 `vite.config.ts` 中配置）
- llama-server: 默认 8080（在设置页可修改）
- 本地 HTTP 服务（浏览器插件）: 默认 8765（在设置页可修改）
