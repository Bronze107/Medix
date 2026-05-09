# Medix 开发踩坑记录

> 记录 Phase 1 ~ Phase 2 开发过程中遇到的技术陷阱和解决方案，供后续开发参考。

---

## 1. Rust 工具链：Windows GNU 缺少 dlltool

**现象**：`cargo check` 报错 `error calling dlltool 'dlltool.exe': program not found`。

**原因**：rustup 默认安装的 `stable-x86_64-pc-windows-gnu` 工具链缺少 MinGW 的 `dlltool.exe`。

**解决**：切换到 MSVC 工具链：

```bash
rustup toolchain install stable-x86_64-pc-windows-msvc
rustup default stable-x86_64-pc-windows-msvc
```

---

## 2. Tauri 构建需要图标文件

**现象**：`cargo check` 报错 `icons/icon.ico not found; required for generating a Windows Resource file`。

**原因**：`tauri-build` 在编译时会检查 `tauri.conf.json` 中 `bundle.icon` 引用的图标文件是否存在。

**解决**：

1. 先用 Node.js 的 `sharp` 生成一个 `icon.png`：

```bash
npm install sharp --no-save
node -e "const s=require('sharp'); s(Buffer.from('<svg>...</svg>')).png().toFile('src-tauri/icons/icon.png')"
```

2. 再用 Tauri CLI 生成全套图标：

```bash
npx tauri icon src-tauri/icons/icon.png
```

---

## 3. kamadak-exif 的 crate 引用名不是依赖名

**现象**：`use kamadak_exif::Reader` 报错 `unresolved import kamadak_exif`，但 `cargo.toml` 中明明写了 `kamadak-exif = "0.6"`。

**原因**：该 crate 的 `Cargo.toml` 中定义了 `[lib] name = "exif"`，所以 Rust 中的引用名是 `exif`，不是 `kamadak_exif`。

**解决**：代码中使用 `exif::Reader`，Cargo.toml 中保持 `kamadak-exif = "0.6"` 不变。

---

## 4. Tauri WebView 的 File.path 永远是 undefined

**现象**：HTML5 拖放获取的 `File` 对象，`file.path` 返回 `undefined`。

**原因**：Tauri 的 WebView 出于安全考虑，不暴露文件的本地绝对路径。

**解决**：不使用 HTML5 `onDrop` + `file.path`，改用 Tauri 原生事件：

```typescript
import { listen } from "@tauri-apps/api/event";

listen("tauri://drag-drop", (event) => {
  const paths = event.payload.paths as string[]; // 绝对路径数组
  // 调用 media_import(paths)
});
```

配套视觉效果：
- `tauri://drag-enter` — 高亮 DropZone
- `tauri://drag-leave` — 取消高亮
- `tauri://drag-drop` — 执行导入

---

## 5. Windows 路径 + convertFileSrc = 无法加载

**现象**：缩略图文件已生成，前端用 `convertFileSrc(path)` 得到 URL，但 `<img>` 不显示图片。

**原因**：
- `convertFileSrc` 在 Windows 上生成 `http://asset.localhost/C%3A%2FUsers%2F...` URL
- `C:` 盘符被 URL 编码为 `C%3A`，WebView2 的 asset protocol 无法正确解码还原为本地路径
- Tauri v2 的 asset protocol 在 Windows 上默认不启用跨盘符访问

**解决**：放弃 `convertFileSrc`，改用 Rust 读取文件返回 base64：

```rust
#[command]
pub fn media_thumbnail(app: AppHandle, id: String) -> Result<String, String> {
    let thumb_path = app.path().app_data_dir()
        .map_err(|e| e.to_string())?
        .join("thumbnails")
        .join(format!("{}_256.jpg", id));

    let bytes = fs::read(&thumb_path).map_err(|e| e.to_string())?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:image/jpeg;base64,{}", b64))
}
```

前端缓存 base64 避免重复 IPC：

```typescript
const thumbCache = new Map<string, string>();
```

---

## 6. Vite 不认识 TypeScript 的 paths 别名

**现象**：`npx tsc --noEmit` 通过，但 `vite build` 报错 `Rollup failed to resolve import "@/lib/tauri"`。

**原因**：`tsconfig.json` 中的 `"@/*": ["src/*"]` 只告诉 TypeScript 编译器如何解析，Vite 的 Rollup 不知道这个映射。

**解决**：在 `vite.config.ts` 中配置 `resolve.alias`：

```typescript
import path from "path";

export default defineConfig({
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "src"),
    },
  },
});
```

---

## 7. image crate 没有 webp-encoder feature

**现象**：Cargo.toml 中写 `features = ["webp", "webp-encoder"]`，`cargo check` 报错 `image does not have that feature`。

**原因**：`image` v0.25 只有 `webp` 解码 feature，没有 `webp-encoder`。WebP 编码需要额外的 `libwebp-sys` 绑定。

**解决**：缩略图先用 JPEG 编码（`image::codecs::jpeg::JpegEncoder`），后续需要 WebP 时再引入 `webp` crate。

---

## 8. Rust 结构体加字段后实例化处也要同步

**现象**：给 `Media` 结构体加了 `thumb_256` 和 `thumb_512` 字段后，`cargo check` 报错 `missing fields thumb_256 and thumb_512 in initializer`。

**原因**：Rust 结构体实例化必须提供所有字段（除非使用 `..Default::default()`）。

**解决**：在 `import.rs` 的 `Media { ... }` 初始化中添加：

```rust
thumb_256: None,
thumb_512: None,
```

---

## 9. Tauri v2 的 WindowEvent 名称变化

**现象**：`WindowEvent::FileDrop(FileDropEvent::Dropped(...))` 编译报错。

**原因**：Tauri v2 中 `FileDrop` 改名为 `DragDrop`，`FileDropEvent` 改名为 `DragDropEvent`，变体 `Dropped` 改名为 `Drop`。

**解决**：

```rust
use tauri::{Manager, WindowEvent};

window.on_window_event(move |event| {
    if let WindowEvent::DragDrop(file_drop) = event {
        if let tauri::DragDropEvent::Drop { paths, position } = file_drop {
            // ...
        }
    }
});
```

> **注意**：实际项目中我们没有用手动监听 `WindowEvent`，因为 Tauri 已经内置自动 emit `tauri://drag-drop` 事件给前端，直接用 `listen` 即可。

---

## 10. base64 crate 需要导入 Engine trait

**现象**：`base64::engine::general_purpose::STANDARD.encode(&bytes)` 报错 `no method named encode found`。

**原因**：`encode` 是 `Engine` trait 的方法，需要显式导入 trait。

**解决**：

```rust
use base64::Engine;
```

---

## 快速参考

| 问题 | 关键词 | 解决 |
|------|--------|------|
| dlltool 缺失 | `parking_lot` 编译失败 | 切换 MSVC 工具链 |
| 图标缺失 | `tauri-build` icon | `npx tauri icon` |
| EXIF 导入 | `kamadak_exif` 找不到 | 代码用 `exif::` |
| 拖放路径 | `file.path` undefined | 用 `tauri://drag-drop` 事件 |
| 本地图片加载 | `convertFileSrc` 黑屏 | Rust 返回 base64 |
| Vite 别名 | `@/` 解析失败 | `resolve.alias` |
| WebP 编码 | `webp-encoder` 不存在 | 先用 JPEG |
| 结构体字段 | missing fields | 同步更新所有初始化处 |
| base64 | `encode` 找不到 | `use base64::Engine;` |
