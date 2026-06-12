#[cfg(test)]
mod tests {
    use crate::export::is_ai_source;

    #[test]
    fn test_is_ai_source() {
        assert!(is_ai_source(Some("ai")));
        assert!(is_ai_source(Some("ai_en")));
        assert!(is_ai_source(Some("ai_zh")));
        assert!(!is_ai_source(Some("manual")));
        assert!(!is_ai_source(Some("imported")));
        assert!(!is_ai_source(None));
        assert!(!is_ai_source(Some("AI"))); // case-sensitive
    }
}
