#!/bin/bash
# Installationsskript für ÖBB Monitor

set -e

echo "🚂 ÖBB Monitor - Installation"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

# Check if cargo is installed
if ! command -v cargo &> /dev/null; then
    echo "❌ Cargo nicht gefunden!"
    echo "Bitte installiere Rust von: https://rustup.rs/"
    exit 1
fi

echo "✓ Rust/Cargo gefunden"
echo ""

# Build and install
echo "📦 Kompiliere und installiere..."
echo "   (stations.json wird in das Binary eingebettet)"
cargo install --path . --force

echo ""
echo "✅ Installation abgeschlossen!"
echo ""
echo "Das Programm wurde installiert nach:"
echo "  $(which oebb-monitor 2>/dev/null || echo '~/.cargo/bin/oebb-monitor')"
echo ""
echo "ℹ️  Die Stationsdaten (844 Bahnhöfe) sind im Binary eingebettet."
echo "   Das Programm funktioniert von überall ohne zusätzliche Dateien!"
echo ""

# Check if ~/.cargo/bin is in PATH
if [[ ":$PATH:" != *":$HOME/.cargo/bin:"* ]]; then
    echo "⚠️  WICHTIG: ~/.cargo/bin ist nicht im PATH!"
    echo ""
    echo "Füge folgende Zeile zu deiner Shell-Konfiguration hinzu:"
    echo ""
    
    if [[ "$SHELL" == *"zsh"* ]]; then
        echo "  echo 'export PATH=\"\$HOME/.cargo/bin:\$PATH\"' >> ~/.zshrc"
        echo "  source ~/.zshrc"
    elif [[ "$SHELL" == *"bash"* ]]; then
        echo "  echo 'export PATH=\"\$HOME/.cargo/bin:\$PATH\"' >> ~/.bashrc"
        echo "  source ~/.bashrc"
    else
        echo "  export PATH=\"\$HOME/.cargo/bin:\$PATH\""
    fi
    echo ""
fi

echo "🎉 Starte das Programm mit: oebb-monitor"
echo ""
echo "Optionen:"
echo "  oebb-monitor         → Normal starten"
echo "  oebb-monitor --debug → Mit Debug-Logging"
echo ""
