#!/bin/bash
# Helper script to run the ÖBB TUI with debug logging

echo "Starting ÖBB TUI in debug mode..."
echo "Debug log will be written to: /tmp/oebb-debug.log"
echo ""
echo "To view logs in real-time (in another terminal):"
echo "  tail -f /tmp/oebb-debug.log"
echo ""

# Clear old log
rm -f /tmp/oebb-debug.log

# Run the app
cargo run --release -- --debug

echo ""
echo "App exited. Debug log available at: /tmp/oebb-debug.log"
