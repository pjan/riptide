# Video Reference Audit — riptide-rs

## Context

riptide-rs is audio-only, permanently. Video downloads will never be
supported. Video references should be kept only where strictly necessary for
parsing TIDAL API responses (TIDAL returns videos in playlists, albums, search
results, and mixes). Everything downstream of parsing — streaming, downloading,
progress tracking, metadata tagging, template rendering for videos — is dead
code and should be removed.

## Guiding Principle

**Parse videos from TIDAL responses. Never download, stream, tag, or render
progress for them.**

## Keep (TIDAL returns videos in API responses, we must parse them)

| Location | Why keep |
|---|---|
| `riptide-core`: `struct Video`, `CatalogEntity::Video`, `ResourceKind::Video` | Core data model — TIDAL's catalog contains videos, we must deserialise them |
| `riptide-provider-tidal`: `TidalVideo`, `convert::video()`, `convert_catalog_item` video detection | Playlists, albums, and search results contain videos |
| `riptide-provider-tidal`: `"video"` URL parsing in `resource.rs` | Must recognise video URLs to reject them cleanly |
| `riptide-provider-tidal`: `client.video()` | `list video <id>` needs this |
| `riptide-provider-tidal`: `resolve()` for `ResourceKind::Video` | `list video` resolves via this path |
| `riptide-cli`: `ListCommand::Video` subcommand + handler | Catalog inspection — shows what exists, does not download |
| `riptide-cli`: search result display (`results.videos`) | Search shows what exists |
| `riptide-config`: rejection tests for `video_quality`, `videos_mode`, `templates.video` | Already correct — documents the intentional exclusion |

## Remove entirely

These are dead code — no future slice will use them.

### `riptide-core`

| What | Notes |
|---|---|
| `VideoQualityRequest` enum + `FromStr` impl | Only used by video streaming path |
| `VideoQuality` enum | Only used by video streaming path |
| `ResolvedQuality::Video` variant | Only used by video streaming path |
| `QualityRequest::Video` variant | Only used by video streaming path |
| `Album::video_count` field | Never used in any download or display decision |

### `riptide-provider-tidal`

| What | Notes |
|---|---|
| `client.video_stream()` | Video streaming — never called |
| `stream::decode_video_manifest()` | Paired with `video_stream` — never called |
| `stream::video_quality_request_label()` + re-export in `lib.rs` | Paired with `video_stream` — never called |
| `provider.rs` `streams()` arm for `ResourceKind::Video` | Return `UnsupportedResourceKind` instead |
| `SearchResults::videos` field | Search display can show videos from the raw response, but this typed field is only needed if we do something with videos beyond listing |

### `riptide-cli` (download path)

| What | Notes |
|---|---|
| `main.rs`: `CatalogEntity::Video(v) => Some(&v.id)` in download ref extraction | Videos should not produce download refs |
| `main.rs`: `ResourceKind::Video => "video"` label in download context | Dead after filtering |
| `main.rs`: `CatalogEntity::Video` → `OperationKind::Track` mapping | Videos should not map to download operations |
| `main.rs`: `CatalogEntity::Video(v)` template context in download | Unreachable after filtering |
| `main.rs`: `CatalogEntity::Video(v)` entity row in download | Unreachable after filtering |
| `main.rs`: `CatalogEntity::Video(_) => "video"` kind label in download | Unreachable after filtering |
| `should_skip_entity` + `VideosMode` filtering | Replaced by hard filter; `VideosMode` is already rejected by config |

### `riptide-progress`

| What | Notes |
|---|---|
| `OperationKind::Video` variant | Never emitted — no video operations |
| `ItemKind::Video` variant | Never emitted — no video download items |
| `QualitySymbol::Video` variant | Never emitted |
| `CountUnit::Videos` variant | Never emitted |
| `label_for_video()` | Dead code |
| Video quality badge logic (`VideoQuality` → "360p"/"720p"/"1080p") | Dead code |
| `indicatif_sink.rs`: `OperationKind::Video` label, collection check, quality symbol | Dead match arms |
| `row.rs`: `OperationKind::Video => CountUnit::Videos` | Dead match arm |

### `riptide-template`

| What | Notes |
|---|---|
| `with_video()` | Never called in download context |
| `from_video()` | Never called in download context |

### `riptide-metadata`

| What | Notes |
|---|---|
| `metadata_from_video()` + re-export | Never tagging video files |

## Filtering point

Filter videos out in `expand_download_entities`:

- Direct video URL → return error: "video downloads are not supported"
- Album/playlist/artist/mix children → filter out any `CatalogEntity::Video`
  before returning

This makes all downstream `CatalogEntity::Video` match arms in the download
path unreachable. Remove them rather than leaving dead branches.

## Impact assessment

The most labour-intensive part is removing `OperationKind::Video`,
`ItemKind::Video`, `QualitySymbol::Video`, and `CountUnit::Videos` from
`riptide-progress` — every exhaustive match across `riptide-progress` and
`riptide-cli/progress/` must be updated. This is mechanical but touches many
files.

Removing `VideoQualityRequest` and `QualityRequest::Video` from `riptide-core`
simplifies the `streams()` trait — the quality parameter always carries audio
quality, and the `QualityRequest` enum can potentially be replaced with
`AudioQualityRequest` directly.

## Summary

- Keep: video parsing and catalog types (`struct Video`, `CatalogEntity::Video`,
  `ResourceKind::Video`, TIDAL API models and conversion)
- Keep: `list video` and search display
- Remove: video streaming, video quality types, video progress tracking,
  video metadata tagging, video template rendering, all download-path
  match arms for video entities
- 1 filtering point: `expand_download_entities` rejects direct video URLs
  and filters video children from collections
