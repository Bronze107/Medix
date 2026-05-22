interface SearchBarProps {
  value: string;
  onChange: (value: string) => void;
  onClear: () => void;
}

function parsePills(query: string) {
  const pills: { prefix: string; label: string; color: string; raw: string }[] = [];
  const lower = query.toLowerCase();

  // tag: with quotes: tag:"black cat"
  const quotedTagMatch = lower.match(/tag:"([^"]+)"/);
  if (quotedTagMatch) {
    pills.push({ prefix: "tag", label: quotedTagMatch[1], color: "blue", raw: quotedTagMatch[0] });
  } else {
    // tag: without quotes: tag:cat dog
    const tagMatch = lower.match(/tag:([^a-z]*[a-z\s|]+?)(?=\s+(?:width|height|date|size):|$)/);
    if (tagMatch) {
      const content = tagMatch[1].trim();
      if (content) {
        pills.push({ prefix: "tag", label: content, color: "blue", raw: tagMatch[0] });
      }
    }
  }

  for (const prefix of ["width", "height", "date", "size"]) {
    const m = lower.match(new RegExp(`${prefix}:(\\S+)`));
    if (m) {
      pills.push({ prefix, label: m[1], color: prefix === "width" || prefix === "height" ? "green" : prefix === "date" ? "yellow" : "purple", raw: m[0] });
    }
  }

  return pills;
}

const pillColors: Record<string, string> = {
  blue: "bg-[var(--color-accent-soft)] text-[var(--color-accent)] border-[var(--color-accent)]/30",
  green: "bg-[var(--color-success-soft)] text-[var(--color-success)] border-[var(--color-success)]/30",
  yellow: "bg-[var(--color-warning-soft)] text-[var(--color-warning)] border-[var(--color-warning)]/30",
  purple: "bg-purple-900/30 text-purple-400 border-purple-800/50",
};

function SearchBar({ value, onChange, onClear }: SearchBarProps) {
  const pills = value ? parsePills(value) : [];

  const removePill = (raw: string) => {
    const idx = value.toLowerCase().indexOf(raw);
    if (idx === -1) return;
    let newVal = value.slice(0, idx) + value.slice(idx + raw.length);
    newVal = newVal.replace(/\s{2,}/g, " ").trim();
    onChange(newVal);
  };

  return (
    <div className="w-full rounded-xl border border-[var(--color-border)] bg-[var(--color-bg-secondary)] px-3 py-2">
      {/* Chips row */}
      {pills.length > 0 && (
        <div className="mb-2 flex flex-wrap gap-1.5">
          {pills.map((p, i) => (
            <span
              key={`${p.prefix}-${i}`}
              className={`inline-flex items-center gap-1 rounded-md border px-1.5 py-0.5 text-[11px] ${pillColors[p.color] || ""}`}
            >
              <span className="font-medium">{p.prefix}:</span>
              <span className="max-w-32 truncate">{p.label}</span>
              <button
                onClick={() => removePill(p.raw)}
                className="ml-0.5 rounded-sm p-px hover:bg-black/20 transition-colors"
              >
                <svg className="h-3 w-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2.5}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="M6 18 18 6M6 6l12 12" />
                </svg>
              </button>
            </span>
          ))}
        </div>
      )}

      {/* Input row */}
      <div className="flex items-center gap-2">
        <svg className="h-4 w-4 flex-shrink-0 text-[var(--color-text-muted)]" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
          <path strokeLinecap="round" strokeLinejoin="round" d="m21 21-5.197-5.197m0 0A7.5 7.5 0 1 0 5.196 5.196a7.5 7.5 0 0 0 10.607 10.607Z" />
        </svg>
        <input
          type="text"
          value={value}
          onChange={(e) => onChange(e.target.value)}
          placeholder={pills.length === 0 ? "搜索... tag:cat | width:>1920 | 橘子猫" : "继续输入搜索条件..."}
          className="min-w-0 flex-1 bg-transparent text-sm text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)]"
        />
        {value && (
          <button
            onClick={onClear}
            className="flex-shrink-0 rounded p-0.5 text-[var(--color-text-muted)] hover:text-[var(--color-text-secondary)] transition-colors"
            title="清除搜索"
          >
            <svg className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M6 18 18 6M6 6l12 12" />
            </svg>
          </button>
        )}
      </div>
    </div>
  );
}

export default SearchBar;
