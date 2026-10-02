/**
 * 衍生关系标记：同时表达两个方向。
 *
 * - 出边（childCount）：这张图衍生出了几张图
 * - 入边（parentCount）：这张图源自几张图
 *
 * 两侧都没有关系时返回 null，由调用方决定是否显示「原始」之类的兜底文案。
 * 外观（定位、背景、文字色）由调用方通过 className 传入。
 */
interface LineageBadgeProps {
  parentCount: number;
  childCount: number;
  /** 传入则渲染为可点击的 button，并自动阻止事件冒泡到卡片 */
  onActivate?: () => void;
  className?: string;
}

/** 拼出 hover 提示文案，只包含实际存在的一侧。 */
export function formatLineageTitle(childCount: number, parentCount: number): string {
  return [
    childCount > 0 ? `衍生出 ${childCount} 张图` : null,
    parentCount > 0 ? `源自 ${parentCount} 张图` : null,
  ]
    .filter(Boolean)
    .join(" · ");
}

/** 出边：指向右下的箭头 */
function ChildIcon() {
  return (
    <svg
      className="h-2.5 w-2.5 shrink-0"
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M5 5l6 6" />
      <path d="M11 6v5H6" />
    </svg>
  );
}

/** 入边：指向左上的箭头 */
function ParentIcon() {
  return (
    <svg
      className="h-2.5 w-2.5 shrink-0"
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M11 11L5 5" />
      <path d="M5 10V5h5" />
    </svg>
  );
}

export function LineageBadge({
  parentCount,
  childCount,
  onActivate,
  className = "",
}: LineageBadgeProps) {
  if (childCount <= 0 && parentCount <= 0) return null;

  const title = formatLineageTitle(childCount, parentCount);
  const base = `inline-flex items-center gap-1.5 text-[11px] font-medium ${className}`;

  const content = (
    <>
      {childCount > 0 && (
        <span className="inline-flex items-center gap-0.5 tabular-nums">
          <ChildIcon />
          {childCount}
        </span>
      )}
      {parentCount > 0 && (
        <span className="inline-flex items-center gap-0.5 tabular-nums">
          <ParentIcon />
          {parentCount}
        </span>
      )}
    </>
  );

  if (onActivate) {
    return (
      <button
        type="button"
        title={title}
        aria-label={title}
        onClick={(e) => {
          e.stopPropagation();
          onActivate();
        }}
        onMouseDown={(e) => e.stopPropagation()}
        onDoubleClick={(e) => e.stopPropagation()}
        className={`${base} cursor-pointer transition-colors`}
      >
        {content}
      </button>
    );
  }

  return (
    <span title={title} className={base}>
      {content}
    </span>
  );
}
