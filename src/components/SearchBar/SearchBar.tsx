interface SearchBarProps {
  value: string;
  onChange: (value: string) => void;
  onClear: () => void;
}

function parsePills(query: string) {
  const pills: { prefix: string; label: string; color: string }[] = [];
  const lower = query.toLowerCase();

  // tag:
  const tagMatch = lower.match(/tag:([^a-z]*[a-z\s|]+?)(?=\s+(?:width|height|date|size):|$)/);
  if (tagMatch) {
    const content = tagMatch[1].trim();
    if (content) {
      pills.push({ prefix: "tag", label: content, color: "blue" });
    }
  }

  // width:
  const widthMatch = lower.match(/width:(\S+)/);
  if (widthMatch) {
    pills.push({ prefix: "width", label: widthMatch[1], color: "green" });
  }

  // height:
  const heightMatch = lower.match(/height:(\S+)/);
  if (heightMatch) {
    pills.push({ prefix: "height", label: heightMatch[1], color: "green" });
  }

  // date:
  const dateMatch = lower.match(/date:(\S+)/);
  if (dateMatch) {
    pills.push({ prefix: "date", label: dateMatch[1], color: "yellow" });
  }

  // size:
  const sizeMatch = lower.match(/size:(\S+)/);
  if (sizeMatch) {
    pills.push({ prefix: "size", label: sizeMatch[1], color: "purple" });
  }

  return pills;
}

const pillColors: Record<string, string> = {
  blue: "bg-[var(--color-accent-soft)] text-[var(--color-accent)] border-[var(--color-accent)]/30",
  green: "bg-green-900/30 text-green-400 border-green-800/50",
  yellow: "bg-yellow-900/30 text-yellow-400 border-yellow-800/50",
  purple: "bg-purple-900/30 text-purple-400 border-purple-800/50",
};

function SearchBar({ value, onChange, onClear }: SearchBarProps) {
  const pills = value ? parsePills(value) : [];

  return (
    <div className="flex items-center gap-2 rounded-lg border border-[var(--color-border)] bg-[var(--color-bg-secondary)] px-3 py-2">
      <svg
        className="h-4 w-4 flex-shrink-0 text-[var(--color-text-muted)]"
        fill="none"
        viewBox="0 0 24 24"
        stroke="currentColor"
        strokeWidth={2}
      >
        <path
          strokeLinecap="round"
          strokeLinejoin="round"
          d="m21 21-5.197-5.197m0 0A7.5 7.5 0 1 0 5.196 5.196a7.5 7.5 0 0 0 10.607 10.607Z"
        />
      </svg>

      <div className="flex flex-1 flex-wrap items-center gap-1.5">
        {pills.map((p, i) => (
          <span
            key={`${p.prefix}-${i}`}
            className={`inline-flex items-center gap-1 rounded border px-1.5 py-0.5 text-[11px] ${pillColors[p.color] || ""}`}
          >
            <span className="font-medium">{p.prefix}:</span>
            <span className="max-w-24 truncate">{p.label}</span>
          </span>
        ))}
        <input
          type="text"
          value={value}
          onChange={(e) => onChange(e.target.value)}
          placeholder={
            pills.length === 0
              ? "搜索... tag:cat dog | width:>1920 | 橘子猫"
              : "添加搜索条件..."
          }
          className="min-w-0 flex-1 bg-transparent text-sm text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)]"
        />
      </div>

      {value && (
        <button
          onClick={onClear}
          className="flex-shrink-0 rounded p-0.5 text-[var(--color-text-muted)] hover:text-[var(--color-text-secondary)]"
          title="清除搜索"
        >
          <svg className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
            <path strokeLinecap="round" strokeLinejoin="round" d="M6 18 18 6M6 6l12 12" />
          </svg>
        </button>
      )}
    </div>
  );
}

export default SearchBar;
