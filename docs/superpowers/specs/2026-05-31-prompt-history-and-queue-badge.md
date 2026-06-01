# 提示词历史 & 队列 Badge

**日期**: 2026-05-31
**状态**: 已实现

---

## 1. 提示词历史

### 存储

- `localStorage` 持久化，跨会话保留
- 两个独立 key：
  - `prompt_history_generate` — AI 生图页面
  - `prompt_history_edit` — AI 编辑对话框

### 数据结构

```typescript
interface PromptEntry {
  prompt: string;
  aspectRatio: string;
  resolution: string;
  time: number; // Date.now()
}
```

### 规则

- 上限 10 条
- 去重：相同 prompt 只保留最新
- 超过 10 条自动淘汰最早的
- 点击 chip 回填 prompt + aspectRatio + resolution
- "清除" 按钮一键清空

### 实现

- `src/hooks/usePromptHistory.ts` — 通用 hook
- AiGenPage：textarea 下方 6 个 chips + 清除
- ImagineDialog：textarea 下方 4 个 chips + 清除

### 设计决策

- 方案 A（内联 chips）优于 dropdown（B）和右侧面板（C）— 直接可见、零摩擦
- 生图历史和编辑历史独立存储，不混淆
- localStorage 而非 sessionStorage — 用户明确要求持久化

---

## 2. 队列计数 Badge

### 实现

- 侧边栏 "AI 生图" 导航项右侧 accent 色计数 pill
- 和回收站计数同模式，颜色区分

### 事件流

```
任务提交 → emit image-queue-updated { remaining }
任务完成 → emit image-queue-updated { remaining }
侧边栏监听 → poll imageQueuePendingCount()
```

### 修复

- 提交时也需要发 `image-queue-updated` 事件（最初只在任务完成时触发，导致 badge 不更新）
