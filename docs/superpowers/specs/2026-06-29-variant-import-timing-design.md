---
title: Variant Import Timing Instrumentation
date: 2026-06-29
status: approved
---

# Variant Import Timing Instrumentation

## Goal

Add coarse-grained timing logs to the `variant_import` Tauri command so we can identify which phase of variant import is the bottleneck (format detection, file copy, metadata extraction, database insert, or thumbnail generation).

## Scope

- Modify `src-tauri/src/commands/variant.rs` only.
- Keep the change minimal and consistent with existing `media/import.rs` timing style.
- No UI changes, no new Tauri events, no new dependencies.

## Instrumented Phases

| Phase      | Covers |
|------------|--------|
| `detect`   | Reading magic bytes and deciding image vs video format. |
| `copy`     | Copying the source file into the `variants/` library folder. |
| `metadata` | Image decode via `image::load_from_memory` or video metadata via `ffprobe`. |
| `db_insert`| Inserting the `Variant` record into SQLite. |
| `thumbnail`| Generating the variant thumbnail (image or video). |
| `total`    | End-to-end duration of the whole command. |

## Output Format

Use `println!` for successful phase completions, matching the style used in `media/import.rs` and `ai/mod.rs`:

```text
[variant_import] <media_id> phase=<name> duration_ms=<N>
[variant_import] <media_id> phase=total duration_ms=<N>
```

Keep `eprintln!` for actual errors and for the existing debug lines that show magic bytes/format detection details.

## Error Path Behavior

If a phase fails, print the elapsed time up to that point so slow failures are also visible:

```text
[variant_import] <media_id> phase=<name> failed after <N>ms: <error>
```

## Implementation Notes

- Use `std::time::Instant` for each phase.
- Reset the timer at the start of each new phase.
- Maintain a single `t_total` at the top of the function.
- The existing `eprintln!` debug logs for magic bytes and format detection can stay; the new `println!` timing line will appear next to them.

## Non-Goals

- No structured/tracing logger integration.
- No frontend progress events.
- No timing for `variant_generate` or other variant commands in this change.
