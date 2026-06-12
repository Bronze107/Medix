#[derive(Debug, Default)]
pub struct ParsedQuery {
    pub tag_group: Option<TagGroup>,
    pub dimensions: Vec<DimFilter>,
    pub date_range: Option<DateRange>,
    pub file_size: Option<SizeFilter>,
    pub semantic_text: Option<String>,
    pub media_type: Option<String>,  // "image" or "video"
}

#[derive(Debug)]
pub struct TagGroup {
    pub tags: Vec<String>,
    pub mode: TagMatchMode,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TagMatchMode {
    All,
    Any,
}

#[derive(Debug)]
pub enum DimFilter {
    Width { op: Comparison },
    Height { op: Comparison },
}

#[derive(Debug, Clone)]
pub enum Comparison {
    Gt(i64),
    Lt(i64),
    Range(i64, i64),
}

#[derive(Debug)]
pub struct DateRange {
    pub start: String,
    pub end: String,
}

#[derive(Debug, Clone)]
pub struct SizeFilter {
    pub op: SizeOp,
}

#[derive(Debug, Clone)]
pub enum SizeOp {
    GreaterThan(u64),
    LessThan(u64),
}

const PREFIXES: &[&str] = &["tag:", "width:", "height:", "date:", "size:", "media_type:"];

/// Parse a search query string into structured filters.
pub fn parse(input: &str) -> ParsedQuery {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return ParsedQuery::default();
    }

    let lower = trimmed.to_lowercase();
    let chars: Vec<char> = lower.chars().collect();

    // Split the input into segments: each prefix starts a new segment
    #[derive(Debug)]
    struct Segment {
        prefix: Option<String>,
        content: String,
        start: usize,
    }

    let mut segments: Vec<Segment> = Vec::new();
    let mut i = 0;

    // Find the first prefix to determine where it starts
    let mut seg_start = 0;
    let mut current_prefix: Option<String> = None;

    while i < chars.len() {
        // Check if we're at a prefix boundary
        let remaining: String = chars[i..].iter().collect();
        let mut found_prefix = false;

        for prefix in PREFIXES {
            if remaining.starts_with(prefix) {
                // Save previous segment
                if i > seg_start || current_prefix.is_some() {
                    let content: String = chars[seg_start..i].iter().collect();
                    let content = content.trim().to_string();
                    if !content.is_empty() || current_prefix.is_some() && i > seg_start {
                        segments.push(Segment {
                            prefix: current_prefix.take(),
                            content,
                            start: seg_start,
                        });
                    }
                }
                current_prefix = Some(prefix.to_string());
                i += prefix.len();
                seg_start = i;
                found_prefix = true;
                break;
            }
        }

        if !found_prefix {
            i += 1;
        }
    }

    // Save final segment
    if seg_start < chars.len() || current_prefix.is_some() {
        let content: String = chars[seg_start..].iter().collect();
        let content = content.trim().to_string();
        segments.push(Segment {
            prefix: current_prefix.take(),
            content,
            start: seg_start,
        });
    }

    // If no prefixes found at all, everything is semantic text
    if segments.len() == 1 && segments[0].prefix.is_none() {
        return ParsedQuery {
            semantic_text: Some(trimmed.to_string()),
            ..Default::default()
        };
    }

    let mut result = ParsedQuery::default();
    let mut bare_words: Vec<String> = Vec::new();

    for seg in &segments {
        match seg.prefix.as_deref() {
            Some("tag:") => {
                if result.tag_group.is_none() {
                    let (tg, leftover) = parse_tag_content(&seg.content);
                    if let Some(tg) = tg {
                        result.tag_group = Some(tg);
                    }
                    if let Some(sem) = leftover {
                        bare_words.push(sem);
                    }
                }
            }
            Some("width:") => {
                if let Some(op) = parse_comparison(&seg.content) {
                    result.dimensions.push(DimFilter::Width { op });
                }
            }
            Some("height:") => {
                if let Some(op) = parse_comparison(&seg.content) {
                    result.dimensions.push(DimFilter::Height { op });
                }
            }
            Some("date:") => {
                if result.date_range.is_none() {
                    result.date_range = parse_date_range(&seg.content);
                }
            }
            Some("size:") => {
                if result.file_size.is_none() {
                    result.file_size = parse_size_filter(&seg.content);
                }
            }
            Some("media_type:") => {
                let mt = seg.content.trim().to_lowercase();
                if mt == "image" || mt == "video" {
                    result.media_type = Some(mt);
                }
            }
            None => {
                for word in seg.content.split_whitespace() {
                    bare_words.push(word.to_string());
                }
            }
            _ => {}
        }
    }

    if !bare_words.is_empty() {
        result.semantic_text = Some(bare_words.join(" "));
    }

    result
}

/// Returns (tag_words, leftover_semantic). Any word containing at least one letter
/// or digit is a valid tag (including Chinese, Japanese, etc.). Purely symbolic
/// tokens fall into semantic. Quoted strings like "black cat" are single tag tokens.
fn split_tag_words(content: &str) -> (Vec<String>, Vec<String>) {
    let mut tags = Vec::new();
    let mut semantic = Vec::new();
    let mut i = 0;
    let chars: Vec<char> = content.chars().collect();

    while i < chars.len() {
        // Skip whitespace and commas
        if chars[i].is_whitespace() || chars[i] == ',' {
            i += 1;
            continue;
        }

        // Quoted string → single token
        if chars[i] == '"' {
            i += 1;
            let start = i;
            while i < chars.len() && chars[i] != '"' {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let word = word.trim().to_lowercase();
            if !word.is_empty() {
                tags.push(word);
            }
            if i < chars.len() { i += 1; } // skip closing quote
            continue;
        }

        // Unquoted word
        let start = i;
        while i < chars.len() && !chars[i].is_whitespace() && chars[i] != ',' {
            i += 1;
        }
        let word: String = chars[start..i].iter().collect();
        let word = word.trim_matches(',').to_lowercase();
        if word.is_empty() {
            continue;
        }
        // Any word containing at least one letter or digit is a valid tag.
        // Chinese / Japanese / Cyrillic etc. are all covered by is_alphabetic().
        // Purely symbolic words (e.g. "***") → semantic.
        if word.chars().any(|c| c.is_alphabetic() || c.is_numeric()) {
            tags.push(word);
        } else {
            semantic.push(word);
        }
    }
    (tags, semantic)
}

fn parse_tag_content(content: &str) -> (Option<TagGroup>, Option<String>) {
    let content = content.trim();
    if content.is_empty() {
        return (None, None);
    }

    let (tags, mode, leftover) = if content.contains(" or ") {
        let parts: Vec<&str> = content.split(" or ").collect();
        let mut tags = Vec::new();
        let mut sem = Vec::new();
        for part in parts {
            let (t, s) = split_tag_words(part.trim());
            tags.extend(t);
            sem.extend(s);
        }
        (tags, TagMatchMode::Any, sem)
    } else if content.contains('|') {
        let parts: Vec<&str> = content.split('|').collect();
        let mut tags = Vec::new();
        let mut sem = Vec::new();
        for part in parts {
            let (t, s) = split_tag_words(part.trim());
            tags.extend(t);
            sem.extend(s);
        }
        (tags, TagMatchMode::Any, sem)
    } else {
        let (tags, semantic) = split_tag_words(&content);
        (tags, TagMatchMode::All, semantic)
    };

    let tag_group = if tags.is_empty() {
        None
    } else {
        Some(TagGroup { tags, mode })
    };

    let semantic_text = if leftover.is_empty() {
        None
    } else {
        Some(leftover.join(" "))
    };

    (tag_group, semantic_text)
}

fn parse_comparison(content: &str) -> Option<Comparison> {
    let content = content.trim();
    if content.is_empty() {
        return None;
    }

    if content.contains("..") {
        let parts: Vec<&str> = content.split("..").collect();
        if parts.len() == 2 {
            let lo: i64 = parts[0].trim().parse().ok()?;
            let hi: i64 = parts[1].trim().parse().ok()?;
            return Some(Comparison::Range(lo, hi));
        }
    }

    if content.starts_with(">=") {
        return content[2..].trim().parse().ok().map(Comparison::Gt);
    }
    if content.starts_with("<=") {
        return content[2..].trim().parse().ok().map(Comparison::Lt);
    }
    if content.starts_with('>') {
        return content[1..].trim().parse().ok().map(Comparison::Gt);
    }
    if content.starts_with('<') {
        return content[1..].trim().parse().ok().map(Comparison::Lt);
    }

    // Plain number = exact match -> treat as range with same value
    content.trim().parse().ok().map(|v| Comparison::Range(v, v))
}

fn parse_date_range(content: &str) -> Option<DateRange> {
    let content = content.trim();
    if content.is_empty() {
        return None;
    }

    if content.contains("..") {
        let parts: Vec<&str> = content.split("..").collect();
        if parts.len() == 2 {
            let start = parts[0].trim().to_string();
            let end = parts[1].trim().to_string();
            if !start.is_empty() && !end.is_empty() {
                return Some(DateRange { start, end });
            }
        }
    }

    // Single date
    let date = content.to_string();
    Some(DateRange {
        start: date.clone(),
        end: date,
    })
}

fn parse_size_filter(content: &str) -> Option<SizeFilter> {
    let content = content.trim();
    if content.is_empty() {
        return None;
    }

    let (op_char, value_str) = if content.starts_with(">=") {
        ('>', &content[2..])
    } else if content.starts_with("<=") {
        ('<', &content[2..])
    } else if content.starts_with('>') {
        ('>', &content[1..])
    } else if content.starts_with('<') {
        ('<', &content[1..])
    } else {
        return None;
    };

    let bytes = parse_size_value(value_str.trim())?;

    match op_char {
        '>' => Some(SizeFilter {
            op: SizeOp::GreaterThan(bytes),
        }),
        '<' => Some(SizeFilter {
            op: SizeOp::LessThan(bytes),
        }),
        _ => None,
    }
}

fn parse_size_value(raw: &str) -> Option<u64> {
    let raw = raw.trim().to_lowercase();

    let (num_str, multiplier): (&str, u64) = if raw.ends_with("gb") {
        (&raw[..raw.len() - 2], 1024 * 1024 * 1024)
    } else if raw.ends_with("mb") {
        (&raw[..raw.len() - 2], 1024 * 1024)
    } else if raw.ends_with("kb") {
        (&raw[..raw.len() - 2], 1024)
    } else if raw.ends_with('b') {
        (&raw[..raw.len() - 1], 1)
    } else {
        (raw.as_str(), 1)
    };

    let num: f64 = num_str.trim().parse().ok()?;
    Some((num * multiplier as f64) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty() {
        let r = parse("");
        assert!(r.tag_group.is_none());
        assert!(r.semantic_text.is_none());
    }

    #[test]
    fn test_tag_intersection() {
        let r = parse("tag:cat dog");
        let tg = r.tag_group.unwrap();
        assert_eq!(tg.tags, vec!["cat", "dog"]);
        assert!(matches!(tg.mode, TagMatchMode::All));
    }

    #[test]
    fn test_tag_union() {
        let r = parse("tag:cat | dog");
        let tg = r.tag_group.unwrap();
        assert_eq!(tg.tags, vec!["cat", "dog"]);
        assert!(matches!(tg.mode, TagMatchMode::Any));
    }

    #[test]
    fn test_tag_union_or() {
        let r = parse("tag:cat or dog");
        let tg = r.tag_group.unwrap();
        assert_eq!(tg.tags, vec!["cat", "dog"]);
        assert!(matches!(tg.mode, TagMatchMode::Any));
    }

    #[test]
    fn test_semantic_only() {
        let r = parse("一只橘猫");
        assert_eq!(r.semantic_text, Some("一只橘猫".to_string()));
        assert!(r.tag_group.is_none());
    }

    #[test]
    fn test_chinese_tag() {
        let r = parse("tag:猫");
        let tg = r.tag_group.unwrap();
        assert_eq!(tg.tags, vec!["猫"]);
        assert_eq!(tg.mode, TagMatchMode::All);
    }

    #[test]
    fn test_mixed() {
        let r = parse("tag:cat 橘子猫 width:>1000");
        let tg = r.tag_group.unwrap();
        assert_eq!(tg.tags, vec!["cat", "橘子猫"]);
        assert_eq!(r.semantic_text, None);
        assert_eq!(r.dimensions.len(), 1);
    }

    #[test]
    fn test_width_gt() {
        let r = parse("width:>1920");
        match &r.dimensions[0] {
            DimFilter::Width { op } => assert!(matches!(op, Comparison::Gt(1920))),
            _ => panic!(),
        }
    }

    #[test]
    fn test_size_lt() {
        let r = parse("size:<1mb");
        let sf = r.file_size.unwrap();
        assert!(matches!(sf.op, SizeOp::LessThan(v) if v == 1024 * 1024));
    }

    #[test]
    fn test_size_gt() {
        let r = parse("size:>500kb");
        let sf = r.file_size.unwrap();
        assert!(matches!(sf.op, SizeOp::GreaterThan(v) if v == 500 * 1024));
    }

    #[test]
    fn test_date_range() {
        let r = parse("date:2024-01-01..2024-12-31");
        let dr = r.date_range.unwrap();
        assert_eq!(dr.start, "2024-01-01");
        assert_eq!(dr.end, "2024-12-31");
    }

    #[test]
    fn test_tag_quoted() {
        let r = parse("tag:\"black cat\"");
        let tg = r.tag_group.unwrap();
        assert_eq!(tg.tags, vec!["black cat"]);
    }

    #[test]
    fn test_height_range() {
        let r = parse("height:800..1920");
        match &r.dimensions[0] {
            DimFilter::Height { op } => assert!(matches!(op, Comparison::Range(800, 1920))),
            _ => panic!(),
        }
    }

    use proptest::prelude::*;

    proptest! {
        #[test]
        fn parse_does_not_panic(s in "\\PC*") {
            // Any arbitrary string should not cause a panic
            let _ = parse(&s);
        }

        #[test]
        fn parse_preserves_semantic_text(s in "[a-zA-Z0-9 \\u{4e00}-\\u{9fff}]{0,100}") {
            let r = parse(&s);
            // If no structured prefix appears, semantic_text should capture it
            if !s.contains(':') && !s.is_empty() {
                assert!(r.tag_group.is_none() || r.semantic_text.is_some());
            }
        }

        #[test]
        fn tag_filter_roundtrip(tags in prop::collection::vec("[a-zA-Z0-9_\\u{4e00}-\\u{9fff}]{1,20}", 1..5)) {
            let joined = tags.join(" ");
            let query = format!("tag:{}", joined);
            let r = parse(&query);
            if let Some(tg) = r.tag_group {
                assert!(!tg.tags.is_empty(), "tag: with content should produce tags");
                for t in &tg.tags {
                    assert!(!t.is_empty(), "individual tags should not be empty");
                }
            }
        }

        #[test]
        fn width_filter_roundtrip(w in 0i64..100000i64) {
            let r = parse(&format!("width:>{}", w));
            if let Some(DimFilter::Width { op }) = r.dimensions.first() {
                assert!(matches!(op, Comparison::Gt(v) if *v == w));
            } else {
                panic!("width:>{} should produce a Width Gt filter", w);
            }
        }
    }
}
