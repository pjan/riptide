# riptide-rs Code Review Action Plan

## Status

Approved decisions, ready to implement.

## Context

Deep code review of the riptide-rs workspace identified 10 actionable findings
across workspace structure, type safety, async correctness, performance, and
maintainability. Each finding was triaged with options; this plan records the
chosen actions and an implementation order that respects dependencies.

## Decisions

| # | Finding | Decision | Crate(s) |
|---|---------|----------|----------|
| 1 | Blocking `std::fs` in async fns | Replace with `tokio::fs::create_dir_all().await` | cli |
| 2 | Double-write in segment downloads | Single-segment fast path; direct write to output | media |
| 3 | Stringly-typed `&str` IDs | Change `TidalClient` methods to accept `&ResourceId` | provider-tidal |
| 4 | `pub mod` + `pub use` dual exposure | Make modules private, keep only `pub use` re-exports | core |
| 5 | Missing `#[non_exhaustive]` | Add to all extensible enums + wildcard arms | core, downstream |
| 6 | Unnecessary `String` allocs in params | Change `get_json` to accept `&[(&str, &str)]` | provider-tidal |
| 7 | `Box::pin` + manual buffer | Replace `append_file` with `tokio::io::copy` (fold into #2) | media |
| 8 | Fragile video detection heuristic | Remove `"quality"` heuristic, rely on `"type"` only | provider-tidal |
| 9 | `LyricsSourceName` duplicates macro | Replace with `non_empty_string_type!` macro | core |
| 10 | Cover data copied unnecessarily | Thread `Arc<[u8]>` to avoid `.to_vec()` | cli |

## Implementation Order

Ordered bottom-up: core types first, then library crates, then CLI. Each step
is independently compilable and testable.

### Phase 1 — Core type housekeeping (no dependents affected)

**Step 1.1 — Private modules in `riptide-core` (Decision 4)**

- File: `crates/riptide-core/src/lib.rs`
- Change `pub mod X` → `mod X` for all 7 modules
- Keep all existing `pub use` re-exports unchanged
- Verify: `cargo check --workspace`

**Step 1.2 — Move `non_empty_string_type!` macro and use it for `LyricsSourceName` (Decision 9)**

- Move the `non_empty_string_type!` macro from `id.rs` to `lib.rs` (before
  module declarations) so it is available crate-wide
- In `stream.rs`: delete the manual `LyricsSourceName` impl (~25 lines) and
  replace with `non_empty_string_type!(LyricsSourceName)`
- Add `pub use stream::LyricsSourceName;` to the existing re-exports in `lib.rs`
  (the macro-generated type gains `Display`, `FromStr`, `TryFrom` for free)
- Verify: existing tests pass, `LyricsSourceName` gains `Display`

**Step 1.3 — Add `#[non_exhaustive]` to extensible enums (Decision 5)**

- Add `#[non_exhaustive]` to these enums in `riptide-core`:
  - `ResourceKind` (`id.rs`)
  - `ContainerKind` (`stream.rs`)
  - `ContainerExtension` (`stream.rs`)
  - `PostProcessStep` (`stream.rs`)
  - `ArtistRole` (`catalog.rs`)
  - `ProviderErrorKind` (`error.rs`)
  - `CollectionKind` (`catalog.rs`)
  - `CatalogEntity` (`catalog.rs`)
- Add wildcard `_ =>` arms to all `match` statements on these enums across the
  workspace. Each wildcard arm should produce a sensible default or
  `unreachable!("new variant not handled")` depending on context.
- Verify: `cargo check --workspace`, `cargo test --workspace`

### Phase 2 — Library crate improvements

**Step 2.1 — Download path refactor (Decisions 2 + 7)**

- File: `crates/riptide-media/src/download.rs`
- Add a single-segment fast path: when `segments.len() == 1`, download directly
  to the output file (skip the part-file indirection)
- For the multi-segment path: replace `append_file` (manual 64KB buffer +
  `Box::pin`) with `tokio::io::copy(&mut input, &mut output)`
- Delete the `append_file` function entirely
- Update the existing test to cover both single- and multi-segment paths
- Verify: `cargo test -p riptide-media`

**Step 2.2 — `TidalClient` accepts `&ResourceId` (Decision 3)**

- File: `crates/riptide-provider-tidal/src/client.rs`
- Change `track(&self, id: &str)`, `video`, `album`, `playlist`, `lyrics` to
  accept `&ResourceId` instead of `&str`
- Update internal calls: use `id.as_str()` in `format!` strings
- Update call sites in `provider.rs` (pass `&reference.id`)
- Update call sites in CLI `main.rs` (pass `&ResourceId` from `ListOne.id` —
  this requires parsing the CLI string into a `ResourceId` at the call site)
- Verify: `cargo check --workspace`, `cargo test --workspace`

**Step 2.3 — `get_json` accepts `&[(&str, &str)]` (Decision 6)**

- File: `crates/riptide-provider-tidal/src/client.rs`
- Change `get_json` parameter from `extra_params: &[(String, String)]` to
  `extra_params: &[(&str, &str)]`
- Update `CacheKey::new` to accept `&[(&str, &str)]` (change its params type
  accordingly, including the internal sort + join)
- Update all call sites (`track_stream`, `video_stream`, `paged_items`,
  `search`, and the `get_json` body itself for `countryCode`)
- Verify: `cargo test -p riptide-provider-tidal`

**Step 2.4 — Remove `"quality"` video heuristic (Decision 8)**

- File: `crates/riptide-provider-tidal/src/client.rs`
- In `convert_catalog_item`: remove `|| item.get("quality").is_some()` from the
  video detection condition
- Add a comment explaining the `"type"` field contract
- Verify: `cargo test -p riptide-provider-tidal`

### Phase 3 — CLI fixes

**Step 3.1 — Replace blocking `fs::create_dir_all` with async (Decision 1)**

- File: `crates/riptide-cli/src/main.rs`
- At line ~1000 (`execute_media_download`): replace `fs::create_dir_all` with
  `tokio::fs::create_dir_all(...).await`
- At line ~1120 (`persist_processed_file`): same replacement
- Verify: `cargo check -p riptide-cli`

**Step 3.2 — Thread `Arc<[u8]>` for cover data (Decision 10)**

- File: `crates/riptide-cli/src/main.rs`
- Change `write_cover_sidecar_with_events` to accept `Option<Arc<[u8]>>` instead
  of `Option<&[u8]>`
- In the `spawn_blocking` closure: clone the `Arc` to move it in, then deref to
  `&[u8]` for `save_cover`
- Update the call site in `download_entity` to pass the `Arc` directly instead
  of `.as_deref()`
- Verify: `cargo check -p riptide-cli`

### Phase 4 — Validation

- `cargo fmt --check --all`
- `cargo clippy --workspace -- -D warnings`
- `cargo test --workspace`

## Commit Strategy

One commit per step (1.1 through 3.2), with a descriptive message referencing
the decision number. This keeps each commit independently reviewable and
revertible. Steps within the same phase can be squashed if preferred.
