use criterion::{black_box, criterion_group, criterion_main, Criterion};
use medix::search::parser;

/// Parse a simple tag query — the most common search pattern.
fn bench_parse_simple(c: &mut Criterion) {
    c.bench_function("parser::parse tag:cat", |b| {
        b.iter(|| parser::parse(black_box("tag:cat")))
    });
}

/// Parse a mixed query with multiple filter types.
fn bench_parse_complex(c: &mut Criterion) {
    c.bench_function("parser::parse tag:cat width:>1920 size:>1mb date:2026-01-01..2026-06-01", |b| {
        b.iter(|| {
            parser::parse(black_box(
                "tag:cat dog width:>1920 size:>1mb date:2026-01-01..2026-06-01",
            ))
        })
    });
}

/// Parse a quoted tag — exercises the regex path.
fn bench_parse_quoted_tag(c: &mut Criterion) {
    c.bench_function("parser::parse tag:\"black cat\"", |b| {
        b.iter(|| parser::parse(black_box("tag:\"black cat\"")))
    });
}

/// Parse Chinese/CJK search text — exercises the semantic text path.
fn bench_parse_chinese(c: &mut Criterion) {
    c.bench_function("parser::parse 一只橘猫", |b| {
        b.iter(|| parser::parse(black_box("一只橘猫坐在窗台上")))
    });
}

criterion_group!(
    parser_benches,
    bench_parse_simple,
    bench_parse_complex,
    bench_parse_quoted_tag,
    bench_parse_chinese,
);
criterion_main!(parser_benches);
