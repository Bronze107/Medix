# Claude Code 项目指引

本项目使用 **Medix** 的协作规范，完整规则见：

**[@AGENTS.md](./AGENTS.md)**

## 设计规范

前端 UI 遵循设计语言文档：**[@docs/design-language.md](./docs/design-language.md)**

所有组件必须使用 CSS 变量（`var(--color-*)`）而非硬编码颜色。禁止使用 `window.confirm()`，统一用 `ConfirmDialog` 组件。