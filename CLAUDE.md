# ÖBB Monitor — CLAUDE.md

## Project Overview

A terminal UI (TUI) application in Rust that streams live ÖBB (Austrian Federal Railways) departure and arrival data via WebSocket. Single binary, single source file (`src/main.rs`).

## Build & Run

```bash
# Development build
cargo build

# Run (development)
cargo run

# Run with debug logging to /tmp/oebb-debug.log
cargo run -- --debug

# Release build + install to ~/.cargo/bin/
./install.sh
# or: cargo install --path .
```

## Architecture

Everything lives in `src/main.rs` (~1060 lines). Key parts:

- **`App` struct** — shared state (train items, station, mode, UI list state). Wrapped in `Arc<Mutex<App>>` and shared between the async WebSocket task and the synchronous UI draw loop.
- **`run_websocket`** — async task that opens 5 parallel WebSocket connections (one per page) to the ÖBB API. Merges results by deduplicating on `item.id`, sorts by scheduled time. Listens on a `reconnect_rx` channel to restart connections when the station or content type changes.
- **Main event loop** — locks `App`, draws UI via ratatui, then handles keyboard events. Sends on `reconnect_tx` when station/mode changes; the mutex guard is dropped before sending to avoid holding it across `.await`.
- **UI rendering** — `ui()` dispatches to `render_main`, `render_station_select`, or `render_train_detail` based on `app.mode`.

## Data Flow

```txt
ÖBB WebSocket API (5 pages)
        ↓ tokio-tungstenite
run_websocket task
        ↓ mpsc channel (page_tx/rx)
App.items (merged, sorted)
        ↓ Arc<Mutex<App>>
ratatui draw loop (100ms tick)
        ↓ crossterm
Terminal
```

## Key Data Structures

```rust
struct TrainItem { id, train, line, product, scheduled, expected,
                   destination, origin, track, sector, remarks,
                   via, prioritized_vias, operator, formation }

struct Formation { wagon_number, icons, sector, destination }
// wagon_number == None → locomotive (rendered as 🚂 Lokomotive)
```

## Formation Icon Mapping

Icons come from the API as string keys. Current mapping in `render_train_detail` (around line 840):

| API key        | Display        |
|----------------|----------------|
| `wlan`         | 📶 WLAN        |
| `bicycle`      | 🚲 Fahrrad     |
| `disabled`     | ♿ Rollstuhl   |
| `bistro`       | 🍽️ Bistro      |
| `motherchild`  | 👪 Familie     |
| `silence`      | 🔇 Ruhe        |
| *(unknown)*    | raw string     |

## App Modes

- `AppMode::Normal` — main train list; digits 1–9 select rows (0 selects 10th), ↑↓ navigate, Enter opens detail
- `AppMode::TrainDetail` — detail popup for selected train; ↑↓ navigate between trains, Esc closes
- `AppMode::StationSelect` — station search popup; typing filters, ↑↓ navigate, Enter selects, Esc cancels

## Known Issues / Future Work

- `event::poll` is a synchronous blocking call inside the Tokio async loop — works with multi-threaded runtime but ideally should use `tokio::task::spawn_blocking` or `EventStream`.
- No terminal cleanup on panic/signal — a panic leaves the terminal in raw mode. A RAII guard (or `std::panic::set_hook`) would fix this.
- Data goes stale in `StationSelect` / `TrainDetail` modes (WebSocket updates are discarded while not in Normal mode). No staleness indicator shown.
- No retry logic for individual WebSocket page connection failures.
- Station list is truncated to 20 results with no indication of how many were omitted.
- `lazy_static` crate could be replaced with `std::sync::LazyLock` (stable since Rust 1.80).

## Dependencies

| Crate               | Purpose                          |
|---------------------|----------------------------------|
| `ratatui`           | TUI framework                    |
| `crossterm`         | Terminal backend / input         |
| `tokio`             | Async runtime                    |
| `tokio-tungstenite` | WebSocket client (with native-tls) |
| `serde` / `serde_json` | JSON deserialization          |
| `futures-util`      | `StreamExt` for WebSocket reads  |
| `chrono`            | DateTime parsing / formatting    |
| `anyhow`            | Error handling                   |
| `lazy_static`       | Global `DebugLogger`             |

## Stations Data

`stations.json` is embedded at compile time via `include_str!`. It maps station IDs → names for all 844 ÖBB stations. The default station is Wien Westbahnhof (ID `8101001`).

## WebSocket URL Format

```txt
wss://meine.oebb.at/abfahrtankunft/webdisplay/web_client/ws/
  ?stationId={id}&contentType={departure|arrival}
  &staticLayout=false&page={1-5}&offset=0
  &ignoreIncident=false&expandAll=false
```
