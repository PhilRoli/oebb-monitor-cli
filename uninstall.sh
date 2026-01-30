#!/bin/bash
# Deinstallationsskript für ÖBB Monitor

echo "🚂 ÖBB Monitor - Deinstallation"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

# Check if installed
if ! command -v oebb-monitor &> /dev/null; then
    echo "⚠️  oebb-monitor ist nicht installiert"
    exit 0
fi

echo "Gefundene Installation:"
echo "  $(which oebb-monitor)"
echo ""

read -p "Möchtest du oebb-monitor deinstallieren? [j/N] " -n 1 -r
echo ""

if [[ $REPLY =~ ^[Jj]$ ]]; then
    cargo uninstall oebb-monitor
    
    echo ""
    echo "✅ oebb-monitor wurde deinstalliert"
    
    # Remove debug log if exists
    if [ -f /tmp/oebb-debug.log ]; then
        read -p "Debug-Log (/tmp/oebb-debug.log) auch löschen? [j/N] " -n 1 -r
        echo ""
        if [[ $REPLY =~ ^[Jj]$ ]]; then
            rm /tmp/oebb-debug.log
            echo "✅ Debug-Log gelöscht"
        fi
    fi
else
    echo "Abgebrochen."
fi
