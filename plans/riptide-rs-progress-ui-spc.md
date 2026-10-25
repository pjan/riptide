# Technical UI Specification for download progress

## 1. Overview

This document specifies the Terminal User Interface (TUI) for the riptide-rs
download command. The UI has five download modes (**Artist, Album, Track,
Playlist, Mix**) and transitions between two primary states (**Downloading**
and **Completed**).

The design uses standard Unicode characters and geometric symbols for a clean,
native-application feel. Terminal colors are used when available.

## 2. Terminal Width & Layout

* **Target width:** 100 characters (4-column layout).
* **Minimum width:** 80 characters (3-column layout — speed column dropped).
* **No plain-text fallback.** The TUI is always rendered; 80 columns is the
  minimum assumption.
* **Width detection:** Query terminal width at startup. Use the 100-char
  layout when width >= 100, the 80-char layout otherwise.
* **Borders:** Standard Unicode box-drawing characters:
    * Horizontal rule: `─` (U+2500). Length matches the active layout width
      (100 or 80 characters).
    * Vertical column divider: `│` (U+2502)
* **Alignment:**
    * Left-aligned: track info, phase labels.
    * Right-aligned: download speeds, file sizes (decimal points stack
      vertically).

## 3. Column Width Tables

### 100-character layout (width >= 100)

| Element | Content | Width |
|---|---|---|
| Leading space | | 1 |
| Col 1 (Quality) | `FLAC 16`, `MP3 320`, `Total`, `-------` | 9 |
| Divider | ` │ ` | 3 |
| Col 2 (Target) | Track info, dynamically truncated | 42 |
| Divider | ` │ ` | 3 |
| Col 3 (Progress) | Progress bar or phase text, right-padded | 28 |
| Divider | ` │ ` | 3 |
| Col 4 (Speed) | Right-aligned, always MB/s, one decimal | 10 |
| Trailing space | | 1 |
| **Total** | | **100** |

### 80-character layout (width 80–99)

| Element | Content | Width |
|---|---|---|
| Leading space | | 1 |
| Col 1 (Quality) | Same as 100-char | 9 |
| Divider | ` │ ` | 3 |
| Col 2 (Target) | Narrower truncation | 35 |
| Divider | ` │ ` | 3 |
| Col 3 (Progress) | Same as 100-char | 28 |
| Trailing space | | 1 |
| **Total** | | **80** |

Col 4 (Speed) is dropped entirely in the 80-char layout.

## 4. Typographic & Iconography System

Do not substitute these characters; use exactly the symbols provided.

### Entity Icons

| Entity | Icon | Codepoint |
|---|---|---|
| Artist | `★` | U+2605 |
| Album | `◎` | U+25CE |
| Track | `♪` | U+266A |
| Playlist | `☰` | U+2630 |
| Mix | `☷` | U+2637 |

### Progress Bar

* Filled block: `▰` (U+25B0)
* Empty block: `▱` (U+25B1)
* Bar width: Exactly 20 characters inside the brackets `[ ]`.
* Percentage is right-aligned within a 4-character field: `  5%`, ` 35%`,
  `100%`.
* Full Col 3 breakdown: `[` (1) + ` ` (1) + 20 bar chars + ` ` (1) + `]`
  (1) + ` ` (1) + 4-char percentage = **28 characters**.

### Spinners & Null Values

* Spinners: Standard Braille patterns (`⠋`, `⠙`, `⠹`, `⠸`, `⠼`, `⠴`, `⠦`,
  `⠧`, `⠇`, `⠏`) for active non-transfer phases. Cycle rate: one frame per
  redraw tick (100ms at 10 Hz = ~1 second per full cycle).
* Null speed: `-------` (7 hyphens, right-aligned in Col 4) when speed is
  not applicable (e.g., tagging).

### Unknown Quality Placeholder

When quality is not yet resolved (before stream resolution), Col 1 shows
`-------` (7 hyphens, right-padded to 9 chars with spaces).

### Completion Receipt Icons

| Purpose | Icon | Codepoint |
|---|---|---|
| Time | `⏱` | U+23F1 |
| Size | `⛁` | U+26C1 |
| Errors/Skipped | `⚠` | U+26A0 |
| Path output | `↳` | U+21B3 |

### Truncation

Always use the Unicode ellipsis `…` (U+2026). Never use three periods.

## 5. Color Mapping

Colors are applied when the terminal supports them. Detection uses
`is_terminal()` on stderr.

| Element | Color |
|---|---|
| Progress bar filled blocks (`▰`) | **Cyan** |
| Speed / byte counters | **Cyan** |
| Spinners (`⠋`, `⠙`, …) | **Yellow** |
| Phase text (`Tagging Metadata...`) | **Yellow** |
| Completion `⏱` / `⛁` lines | Default (no color) |
| `⚠` line when errors = 0 | **Green** |
| `⚠` line when errors > 0 | **Red** |
| Failed track list | **Red** |
| Header entity icon (`★`, `◎`, …) | **Bold** |
| Quality in Col 1 | Default (no color) |
| Dividers (`│`) and borders (`─`) | **Dim** |

## 6. State 1: Downloading (Active UI)

This is an interactive, dynamically updating view. It consists of a Header, a
Global Progress row (for multi-item operations), and a dynamically sized Worker
Pool.

### Header

Printed above the bordered area, not inside it.

| Mode | Header format |
|---|---|
| Artist | `Downloading: ★ Artist Name` |
| Album | `Downloading: ◎ [Artist Name] Album Title` |
| Track | `Downloading: ♪ [Artist Name] Track Title` |
| Playlist | `Downloading: ☰ Playlist Title` |
| Mix | `Downloading: ☷ Mix Title` |

Artist context is shown in brackets when the artist is not the primary
entity. Playlist and Mix are named collections without artist context in the
header.

### Global Progress Row (Total)

Shown for Album, Artist, Playlist, and Mix modes. Not shown for Track mode.

Col 1 shows `Total` (right-padded to 9). Col 2 shows the item summary.
Col 3 shows the aggregate progress bar. Col 4 shows the aggregate speed
(100-char layout only).

#### Col 2 summary text by mode

| Mode | Col 2 text | Example |
|---|---|---|
| Artist | `N Albums // M Tracks` | `2 Albums // 24 Tracks` |
| Album | `N Tracks` | `14 Tracks` |
| Playlist | `N Tracks` | `45 Tracks` |
| Mix | `N Tracks` | `30 Tracks` |

When the total count is not yet known (e.g., artist albums still being
enumerated), show `? Tracks` or `? Albums // ? Tracks`.

#### Total progress bar

The Total row progress bar is **item-based**: percentage =
`completed_items / total_items`. Per-item rows show byte-level granularity;
the Total row shows operation-level progress.

#### Aggregate speed

The aggregate speed is the sum of per-item instantaneous rates. Per-item
rate is derived from consecutive `TransferUpdate` deltas (delta bytes /
delta time). The sum across all active items gives the aggregate.

### Worker Pool

* Rows appear when an item transitions from `Queued` to its first active
  phase (`ResolvingMetadata` or later). Queued items are **not shown**.
* As items finish, their rows are removed immediately. The pool shrinks
  dynamically — if 5 items are configured but only 2 remain, 2 rows are
  shown.

### Col 2 Formatting by Mode

| Mode | Col 2 format | Rationale |
|---|---|---|
| Artist | `[Album Name…] TT Track Title` | Album context needed |
| Album | `TT Track Title` | Album name is in the header |
| Track | `Track Title` | No number needed for single track |
| Playlist | `[Artist Name…] Track Title` | Artist context needed |
| Mix | `[Artist Name…] Track Title` | Same as playlist |

`TT` = zero-padded two-digit track number. Disc numbers are not shown;
the album bracket provides disambiguation for multi-disc albums.

### Col 2 Truncation Rules

When the target string exceeds Col 2 width:

1. Truncate the bracketed text first, retaining a minimum of 5 characters
   before the `…`. Example: `[The Co…] 01 Everlong`.
2. If the string still exceeds the width, truncate the track title.
   Example: `[The Colour and the Shape] 01 This Title Is Too Lo…`.
3. Always append `…` (U+2026) at the truncation point.

### Col 3: Progress vs. Phase

Col 3 is always exactly 28 characters. It holds one of two content types:

* **Progress bar** (during `Downloading`/`Copying` phases):
  `[ ▰▰▰▰▰▰▱▱▱▱▱▱▱▱▱▱▱▱▱▱ ] 35%`
* **Phase text** (during all other active phases):
  `⠋ Tagging Metadata...`

Phase text is right-padded with spaces to fill the full 28-char width so
that the `│` divider position never shifts.

### Phase Label Mapping

| Phase | Display label |
|---|---|
| `ResolvingMetadata` | `Resolving Metadata...` |
| `ResolvingStream` | `Resolving Stream...` |
| `ResolvingMedia` | `Resolving Media...` |
| `PostProcessing` | `Post-Processing...` |
| `Tagging` | `Tagging Metadata...` |
| `Persisting` | `Saving File...` |
| `WritingCover` | `Storing Album Cover...` |
| `WritingLyrics` | `Storing Lyrics...` |
| `WritingPlaylist` | `Writing Playlist...` |

`Downloading` and `Copying` use the progress bar, not a phase label.
`Queued` items are not rendered (they appear only once they enter an active
phase).

## 7. State 2: Completed (Static Receipt)

When the operation finishes, the active UI is **cleared from the terminal**
(cursor moves up, lines are erased via `MultiProgress::clear()`). The static
receipt is printed in its place so that terminal scrollback shows the header
followed directly by the receipt — a clean narrative with no ephemeral
download state.

### Receipt Header

The header changes from `Downloading:` to `Download completed:`:

| Mode | Header format |
|---|---|
| Artist | `Download completed: ★ Artist Name` |
| Album | `Download completed: ◎ [Artist Name] Album Title` |
| Track | `Download completed: ♪ [Artist Name] Track Title` |
| Playlist | `Download completed: ☰ Playlist Title` |
| Mix | `Download completed: ☷ Mix Title` |

### Receipt Body

```
Download completed: ★ Foo Fighters

────────────────────────────────────────────────────────────────────────────────────────────────────
 ⏱ 05:12 (Average 3.5 MB/s)
 ⛁ 1423.5 MB
 ⚠ No errors / skipped
────────────────────────────────────────────────────────────────────────────────────────────────────
 ↳ Saved to: ~/Music/Foo Fighters/
```

* **Line 1:** `⏱ MM:SS (Average XX.X MB/s)` — elapsed time and average
  speed (`total_bytes / elapsed`). Time format: `MM:SS` for < 60 minutes,
  `H:MM:SS` for >= 60 minutes. Speed is always in MB/s with one decimal
  place.
* **Line 2:** `⛁ XXXX.X MB` — total bytes transferred. Always displayed
  in MB with one decimal place.
* **Line 3:** `⚠ No errors / skipped` (green) or `⚠ X errors / skipped`
  (red).
* **Border:** Bottom `─` rule (width matches layout).
* **Footer:** `↳ Saved to: /path/` — printed **below** the bottom border,
  outside the boxed area. This line may exceed the column width constraint
  (paths can be long). The path comes from `EffectiveDownloadOptions.root`
  (passed to the renderer at construction time).

### Failed Track Detail

When `show_failures` is `true` and errors > 0, a compact list of failed
track names is printed after the `↳` line:

```
 ↳ Saved to: ~/Music/Foo Fighters/
 ⚠ Failed: 03 Hey, Johnny Park!, 07 My Hero, 12 New Way Home
```

Failed item labels are extracted from `ProgressState.completed` records
with `ItemOutcome::Failed` before deriving the summary.

When `show_failures` is `false`, only the error count is shown in the
`⚠` line — no supplemental list.

When there are zero errors, no failed list is printed regardless of
`show_failures`.

## 8. Data Model Requirements

### `ProgressState` additions

* `total_bytes_transferred: u64` — accumulated from each item's last
  `TransferProgress.bytes_done` when `ItemFinished` is reduced. The reducer
  snapshots `item.transfer.map(|t| t.bytes_done).unwrap_or(0)` and adds it
  to the running total before discarding the item.

* Per-item rate tracking in `ItemState`:
    * `prev_bytes: u64` — bytes_done from the previous `TransferUpdate`.
    * `prev_instant: Instant` — timestamp of the previous `TransferUpdate`.
    * `current_rate: f64` — bytes/second, recomputed on each
      `TransferUpdate` as `(bytes_done - prev_bytes) / elapsed_seconds`.

  **Timestamp injection:** To keep the reducer testable with synthetic
  times, `reduce()` accepts an `at: Instant` parameter (or the reducer
  captures `Instant::now()` internally with an overridable clock trait for
  tests). The simpler approach: `reduce()` takes `Instant::now()` internally
  and tests use real time with small sleeps, or the rate computation is
  extracted to a pure function tested with synthetic inputs.

### `ProgressSummary` additions

* `total_bytes: u64` — total bytes transferred across all completed items.
* Average speed is derived by the renderer: `total_bytes / elapsed`.
* `failed_labels: Vec<String>` — display labels of items that finished with
  `ItemOutcome::Failed`. Populated by iterating `ProgressState.completed`
  when building the summary.

### Renderer construction

The renderer receives at construction time:

* `output_root: PathBuf` — from `EffectiveDownloadOptions.root`, for the
  `↳ Saved to:` footer.
* `show_failures: bool` — when `true` and errors > 0, the receipt appends
  the `⚠ Failed: ...` line listing failed track names. When `false`, only
  the error count is shown. The CLI sets this to `true` when `--skip-errors`
  is active (partial failures are expected and the user needs to know what
  failed), but the renderer has no awareness of `--skip-errors` itself.

These are not part of the progress event model.

## Examples

### Artist — Downloading

```
Downloading: ★ Foo Fighters

────────────────────────────────────────────────────────────────────────────────────────────────────
 Total     │ 2 Albums // 24 Tracks                       │ [ ▰▰▰▰▰▰▱▱▱▱▱▱▱▱▱▱▱▱▱▱ ]  35% │ 18.5 MB/s
────────────────────────────────────────────────────────────────────────────────────────────────────
 FLAC 16   │ [The Colour...] 01 Everlong                 │ [ ▰▰▰▰▰▰▱▱▱▱▱▱▱▱▱▱▱▱▱▱ ]  30% │  3.2 MB/s
 FLAC 16   │ [The Colour...] 02 Monkey Wrench            │ ⠋ Tagging Metadata...        │   -------
 MP3 320   │ [Wasting Light] 01 Rope                     │ ⠙ Storing Album Cover...     │   -------
 MP3 320   │ [Wasting Light] 02 Walk                     │ [ ▰▰▰▰▰▰▰▰▱▱▱▱▱▱▱▱▱▱▱▱ ]  40% │ 12.3 MB/s
────────────────────────────────────────────────────────────────────────────────────────────────────
```

### Artist — Completed

```
Download completed: ★ Foo Fighters

────────────────────────────────────────────────────────────────────────────────────────────────────
 ⏱ 05:12 (Average 3.5 MB/s)
 ⛁ 1423.5 MB
 ⚠ No errors / skipped
────────────────────────────────────────────────────────────────────────────────────────────────────
 ↳ Saved to: ~/Music/Foo Fighters/
```

### Album — Downloading

```
Downloading: ◎ [Foo Fighters] The Colour and the Shape

────────────────────────────────────────────────────────────────────────────────────────────────────
 Total     │ 14 Tracks                                   │ [ ▰▰▰▰▰▰▱▱▱▱▱▱▱▱▱▱▱▱▱▱ ]  35% │ 18.5 MB/s
────────────────────────────────────────────────────────────────────────────────────────────────────
 FLAC 16   │ 01 Doll                                     │ [ ▰▰▰▰▰▰▱▱▱▱▱▱▱▱▱▱▱▱▱▱ ]  30% │  3.2 MB/s
 FLAC 16   │ 02 Monkey Wrench                            │ ⠋ Tagging Metadata...        │   -------
 FLAC 16   │ 03 Hey, Johnny Park!                        │ ⠙ Storing Album Cover...     │   -------
 FLAC 16   │ 04 My Poor Brain                            │ [ ▰▰▰▰▰▰▰▰▱▱▱▱▱▱▱▱▱▱▱▱ ]  40% │ 12.3 MB/s
────────────────────────────────────────────────────────────────────────────────────────────────────
```

### Album — Completed

```
Download completed: ◎ [Foo Fighters] The Colour and the Shape

────────────────────────────────────────────────────────────────────────────────────────────────────
 ⏱ 01:12 (Average 3.2 MB/s)
 ⛁ 423.5 MB
 ⚠ No errors / skipped
────────────────────────────────────────────────────────────────────────────────────────────────────
 ↳ Saved to: ~/Music/Foo Fighters/The Colour and the Shape/
```

### Track — Downloading

```
Downloading: ♪ [Foo Fighters] Monkey Wrench

────────────────────────────────────────────────────────────────────────────────────────────────────
 FLAC 16   │ Monkey Wrench                               │ [ ▰▰▰▰▰▰▰▰▰▰▰▱▱▱▱▱▱▱▱▱ ]  55% │  3.2 MB/s
────────────────────────────────────────────────────────────────────────────────────────────────────
```

### Track — Completed

```
Download completed: ♪ [Foo Fighters] Monkey Wrench

────────────────────────────────────────────────────────────────────────────────────────────────────
 ⏱ 00:23 (Average 3.2 MB/s)
 ⛁ 43.5 MB
 ⚠ No errors / skipped
────────────────────────────────────────────────────────────────────────────────────────────────────
 ↳ Saved to: ~/Music/Foo Fighters/The Colour and the Shape/02 Monkey Wrench.flac
```

### Playlist — Downloading

```
Downloading: ☰ Ultimate 90s Rock

────────────────────────────────────────────────────────────────────────────────────────────────────
 Total     │ 45 Tracks                                   │ [ ▰▰▰▰▰▰▱▱▱▱▱▱▱▱▱▱▱▱▱▱ ]  35% │ 18.5 MB/s
────────────────────────────────────────────────────────────────────────────────────────────────────
 FLAC 16   │ [Nirvana] Smells Like Teen Spirit           │ [ ▰▰▰▰▰▰▱▱▱▱▱▱▱▱▱▱▱▱▱▱ ]  30% │  3.2 MB/s
 FLAC 16   │ [Pearl Jam] Alive                           │ ⠋ Tagging Metadata...        │   -------
 MP3 320   │ [Soundgarden] Black Hole Sun                │ ⠙ Storing Album Cover...     │   -------
 MP3 320   │ [Alice In Chains] Would?                    │ [ ▰▰▰▰▰▰▰▰▱▱▱▱▱▱▱▱▱▱▱▱ ]  40% │ 12.3 MB/s
────────────────────────────────────────────────────────────────────────────────────────────────────
```

### Playlist — Completed (with failures)

```
Download completed: ☰ Ultimate 90s Rock

────────────────────────────────────────────────────────────────────────────────────────────────────
 ⏱ 14:22 (Average 12.1 MB/s)
 ⛁ 1845.2 MB
 ⚠ 3 errors / skipped
────────────────────────────────────────────────────────────────────────────────────────────────────
 ↳ Saved to: ~/Music/Playlists/Ultimate 90s Rock/
 ⚠ Failed: 12 Under the Bridge, 28 Bullet with Butterfly Wings, 41 Zero
```

### Mix — Downloading

```
Downloading: ☷ Chill Electronic

────────────────────────────────────────────────────────────────────────────────────────────────────
 Total     │ 30 Tracks                                   │ [ ▰▰▰▰▰▰▱▱▱▱▱▱▱▱▱▱▱▱▱▱ ]  35% │ 18.5 MB/s
────────────────────────────────────────────────────────────────────────────────────────────────────
 FLAC 16   │ [Bonobo] Kerala                             │ [ ▰▰▰▰▰▰▱▱▱▱▱▱▱▱▱▱▱▱▱▱ ]  30% │  3.2 MB/s
 FLAC 16   │ [Tycho] Awake                               │ ⠋ Tagging Metadata...        │   -------
 MP3 320   │ [Boards of Canada] Dayvan Cowboy            │ [ ▰▰▰▰▰▰▰▰▱▱▱▱▱▱▱▱▱▱▱▱ ]  40% │ 12.3 MB/s
────────────────────────────────────────────────────────────────────────────────────────────────────
```

### Mix — Completed

```
Download completed: ☷ Chill Electronic

────────────────────────────────────────────────────────────────────────────────────────────────────
 ⏱ 08:45 (Average 5.2 MB/s)
 ⛁ 892.3 MB
 ⚠ No errors / skipped
────────────────────────────────────────────────────────────────────────────────────────────────────
 ↳ Saved to: ~/Music/Mixes/Chill Electronic/
```
