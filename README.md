# ÖBB Monitor

Terminal UI for live ÖBB (Austrian Federal Railways) departure and arrival data, streamed via WebSocket.

## Installation

### Homebrew

```bash
brew tap philroli/oebb-monitor
brew install oebb-monitor
```

### Build from source

Requires Rust 1.80+:

```bash
git clone https://github.com/philroli/oebb-monitor
cd oebb-monitor
cargo install --path .
```

## Usage

```bash
oebb-monitor            # start at Wien Westbahnhof (default)
oebb-monitor --version  # print version
oebb-monitor --debug    # write debug log to /tmp/oebb-debug.log
```

### Keybindings

#### Main view

| Key | Action |
| --- | --- |
| `1`–`9`, `0` | Open detail for trains 1–10 |
| `↑` / `↓` | Move selection |
| `Enter` | Open detail view |
| `A` | Switch to arrivals |
| `D` | Switch to departures |
| `S` | Search / change station |
| `R` | Reconnect and refresh |
| `Q` | Quit |

#### Detail view

| Key | Action |
| --- | --- |
| `↑` / `↓` | Previous / next train |
| `PgUp` / `PgDn` | Scroll content |
| `Esc` / `Q` | Close |

#### Station search

| Key | Action |
| --- | --- |
| Type | Filter by name |
| `↑` / `↓` | Navigate results |
| `Enter` | Select station |
| `Esc` | Cancel |

## Features

- Live departures and arrivals for all 844 ÖBB stations
- Delay indicator with colour coding: on time (green), up to 5 min late (yellow), over 5 min late (red)
- Train detail view: intermediate stops, formation with wagon amenities (Wi-Fi, bicycle, wheelchair, bistro), operator, remarks
- Data stays live regardless of which mode is active (detail view, station search)
- Parallel loading across 5 WebSocket pages for full coverage
- Terminal cleaned up automatically on panic

## Debugging

```bash
oebb-monitor --debug
# in a second terminal:
tail -f /tmp/oebb-debug.log
```

The log captures WebSocket events, reconnect signals, item merges, and key input.

## Architecture

Single-binary Rust application (~1100 lines, `src/main.rs`).

| Crate | Version | Purpose |
| --- | --- | --- |
| ratatui | 0.30 | TUI framework |
| crossterm | 0.29 | Terminal backend |
| tokio | 1 | Async runtime |
| tokio-tungstenite | 0.29 | WebSocket client |
| serde / serde_json | 1 | JSON deserialisation |
| chrono | 0.4 | Time formatting |
| anyhow | 1 | Error handling |

Data source: `wss://meine.oebb.at/abfahrtankunft/webdisplay/web_client/ws/`

## License

MIT
