# ÖBB Monitor 🚂

Ein modernes Terminal User Interface (TUI) zur Anzeige von Echtzeit-Abfahrts- und Ankunftsdaten der Österreichischen Bundesbahnen (ÖBB).

## Features

- 🚉 **844 österreichische Bahnhöfe** - Wechseln Sie zwischen allen ÖBB-Stationen
- 📊 **Live-Updates** - Echtzeit-Daten über WebSocket-Verbindung
- 📄 **Mehrere Seiten** - Lädt bis zu 5 Seiten parallel für mehr Züge
- ⏱️ **Verspätungsanzeige** - Farbcodierte Darstellung von Verspätungen
  - Grün: Pünktlich
  - Gelb: Verspätung bis 5 Minuten
  - Rot: Verspätung über 5 Minuten
- 🔄 **Abfahrt/Ankunft** - Einfacher Wechsel zwischen beiden Ansichten
- 🔍 **Station suchen** - Interaktive Stationsauswahl mit Suchfunktion
- 🎨 **Schöne TUI** - Erstellt mit [Ratatui](https://ratatui.rs/)
- 📍 **Gleis & Sektor** - Anzeige von Gleis und Bahnsteigabschnitt
- 💬 **Hinweise** - Wichtige Informationen und Baustellenmeldungen
- 🚂 **Zug-Details** - Detailansicht mit:
  - 🗺️ Alle Zwischenhalte
  - 🚃 Zugformation (Wagennummern)
  - 📶 Ausstattung (WLAN, Fahrrad, Rollstuhl, Bistro)
  - 👔 Betreiber-Information

## Installation

### Voraussetzungen

- Rust (1.70+)
- Cargo

Falls Rust noch nicht installiert ist:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Schnellinstallation

```bash
cd oebb-monitor
./install.sh
```

Das Script:

- Kompiliert das Programm im Release-Modus
- Installiert es nach `~/.cargo/bin/oebb-monitor`
- Prüft ob `~/.cargo/bin` im PATH ist

### Manuelle Installation

```bash
cd oebb-monitor
cargo install --path .
```

### Nach der Installation

Das Programm ist nun systemweit verfügbar:

```bash
# Normal starten
oebb-monitor

# Mit Debug-Logging
oebb-monitor --debug
```

### Deinstallation

```bash
cargo uninstall oebb-monitor
```

### PATH-Konfiguration

Falls `oebb-monitor: command not found` erscheint, füge `~/.cargo/bin` zum PATH hinzu:

**Zsh (macOS Standard):**

```bash
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc
```

**Bash:**

```bash
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc
```

## Debugging

If you experience issues (e.g., station switching not working), enable debug mode:

```bash
# Run with debug flag
cargo run --release -- --debug

# Or use the helper script
./run-debug.sh

# In another terminal, watch the debug log
tail -f /tmp/oebb-debug.log
```

The debug log shows:

- WebSocket connection attempts and results
- Data received from each page
- Reconnect signals when station/mode changes
- Item count changes and merging
- All user interactions (key presses, station selection)

Debug output is written to `/tmp/oebb-debug.log`

## Build

### Voraussetzungen

- Rust (1.70+)
- Cargo

## Build

Für Entwicklung:

### Voraussetzungen

- Rust (1.70+)
- Cargo

### Build

```bash
cd oebb-monitor
cargo build --release
```

### Run

```bash
cargo run --release
```

Oder das kompilierte Binary direkt starten:

```bash
./target/release/oebb-tui
```

## Bedienung

### Hauptansicht

- **1-9** - Zug 1-9 auswählen (direkt)
- **↑/↓** - Zug auswählen (navigieren)
- **Enter** - Detailansicht des ausgewählten Zugs öffnen
- **A** - Wechsel zu **A**nkünften
- **D** - Wechsel zu Abfahrten (**D**epartures)
- **S** - **S**tation auswählen
- **Q** - Programm beenden (**Q**uit)

### Detailansicht

- **↑/↓** - Vorheriger/Nächster Zug
- **Esc** - Zurück zur Hauptansicht

### Stationsauswahl

- **Tippen** - Station suchen (Filtern nach Name)
- **↑/↓** - In der Liste navigieren
- **Enter** - Station auswählen
- **Esc** - Abbrechen

## Technische Details

### Architektur

- **Async Runtime**: Tokio
- **WebSocket Client**: tokio-tungstenite mit native-tls
- **TUI Framework**: Ratatui 0.29
- **Terminal Backend**: Crossterm 0.28
- **Multi-Page Loading**: 5 parallele WebSocket-Verbindungen für maximale Datenabdeckung

### Datenquelle

Das Programm verbindet sich mit der offiziellen ÖBB WebSocket API:

```txt
wss://meine.oebb.at/abfahrtankunft/webdisplay/web_client/ws/
```

### Projekt-Struktur

```txt
oebb-monitor/
├── src/
│   └── main.rs          # Hauptprogramm (TUI + WebSocket)
├── stations.json        # Liste aller 844 ÖBB-Stationen
├── Cargo.toml           # Dependencies
└── README.md            # Diese Datei
```

## Python-Version

Im Projekt ist auch eine funktionsfähige Python-Version enthalten:

```bash
# Virtual Environment aktivieren
source venv/bin/activate

# Python-Version starten
python oebb-departures.py
```

Die Python-Version bietet die gleichen Features, ist aber weniger performant als die Rust-Version.

## Dependencies

```toml
ratatui = "0.29"          # TUI Framework
crossterm = "0.28"         # Terminal Manipulation
tokio = "1"                # Async Runtime
tokio-tungstenite = "0.24" # WebSocket Client
serde = "1.0"              # Serialization
serde_json = "1.0"         # JSON Parsing
futures-util = "0.3"       # Async Utilities
chrono = "0.4"             # DateTime Handling
anyhow = "1.0"             # Error Handling
```

## Screenshots

```txt
┌─────────────────────────────────────────────────────────────┐
│              🚂 ABFAHRTEN - Wien Westbahnhof                │
└─────────────────────────────────────────────────────────────┘
┌Züge─────────────────────────────────────────────────────────┐
│ZEIT  IST   VERSP.  ZUG    LINIE   ZIEL              GLEIS   │
│03:23 03:25 +2      1600   REX51   St.Pölten Hbf     5       │
│03:48 -     -       9120   CJX5    Kleinreifling     3       │
│...                                                           │
└─────────────────────────────────────────────────────────────┘
```

## Lizenz

MIT License

## Autor

Erstellt für die Überwachung der ÖBB-Züge in Echtzeit.

## Hinweise

- Die App benötigt eine aktive Internetverbindung
- WebSocket-Verbindung wird automatisch neu aufgebaut bei Unterbrechungen
- Daten werden direkt von der offiziellen ÖBB-API bezogen
