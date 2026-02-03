#!/bin/bash
# Take a screenshot from Vast.ai GPU instance
# Usage: ./vast-screenshot.sh [--restart] [--lua FILE] [output.png]
#
# Options:
#   --restart      Kill and restart server/renderer (for TS/Rust changes)
#   --lua FILE     Lua file to load (relative to Mittens/, e.g. examples/my_script.lua)
#                  Default: project/lymph_bath.lua
#
# Note: Lua changes require --restart to take effect (server loads file at startup)

set -e

RESTART=false
LUA_FILE="project/lymph_bath.lua"
OUTPUT="screenshot_$(date +%H%M%S).png"

while [[ $# -gt 0 ]]; do
  case $1 in
    --restart) RESTART=true; shift ;;
    --lua) LUA_FILE="$2"; shift 2 ;;
    *) OUTPUT="$1"; shift ;;
  esac
done

export VAST_API_KEY=$(cat ~/.config/vastai/vast_api_key)

# Check for running instance first
echo "🔍 Checking for running instance..."
INSTANCE_INFO=$(~/.local/bin/vastai show instances --raw | jq -r '.[] | select(.actual_status == "running") | "\(.ssh_host) \(.ssh_port) \(.id)"' | head -1)

if [ -z "$INSTANCE_INFO" ]; then
  echo "❌ No running Vast.ai instance found"
  echo ""
  echo "Start one with:"
  echo "  vastai search offers 'gpu_ram>=4 reliability>0.95 dph<0.15' --order 'dph'"
  echo "  vastai create instance OFFER_ID --image nvidia/cuda:12.2.0-runtime-ubuntu22.04 --disk 20 --ssh"
  exit 1
fi

SSH_HOST=$(echo $INSTANCE_INFO | cut -d' ' -f1)
SSH_PORT=$(echo $INSTANCE_INFO | cut -d' ' -f2)
INSTANCE_ID=$(echo $INSTANCE_INFO | cut -d' ' -f3)
SSH_CMD="ssh -o StrictHostKeyChecking=no -o ConnectTimeout=15 -p $SSH_PORT root@$SSH_HOST"

echo "📡 Found instance $INSTANCE_ID at $SSH_HOST:$SSH_PORT"

if [ "$RESTART" = true ]; then
  echo "🔄 Restarting services with Lua file: $LUA_FILE"
  $SSH_CMD "bash -s" << EOF
pkill -f scriptcad-server 2>/dev/null || true
pkill -f vite 2>/dev/null || true
sleep 1

export LD_LIBRARY_PATH=\$(find /root/Mittens -name "libmanifoldc.so" -path "*/out/lib/*" 2>/dev/null | head -1 | xargs dirname)
cd /root/Mittens && git pull

# Server runs from ./server dir with relative path to project  
export LD_LIBRARY_PATH=\$(find /root/Mittens -name "libmanifoldc.so" -path "*/out/lib/*" 2>/dev/null | head -1 | xargs dirname)
(cd /root/Mittens/server && ./target/release/scriptcad-server ../$LUA_FILE > /tmp/server.log 2>&1) &
(cd /root/Mittens/renderer && npm run dev > /tmp/renderer.log 2>&1) &
sleep 4
echo "✓ Services restarted with $LUA_FILE"
EOF
fi

echo "📸 Taking screenshot..."
$SSH_CMD 'bash -s' << 'EOF'
export DISPLAY=:99
cd /root
node -e "
const puppeteer = require('puppeteer');
(async () => {
  const browser = await puppeteer.launch({
    headless: 'new',
    args: ['--no-sandbox', '--disable-setuid-sandbox', '--disable-dev-shm-usage']
  });
  const page = await browser.newPage();
  await page.setViewport({ width: 1920, height: 1080 });
  await page.goto('http://localhost:3000', { waitUntil: 'load', timeout: 15000 });
  await new Promise(r => setTimeout(r, 5000));
  await page.screenshot({ path: '/tmp/vast_screenshot.png' });
  await browser.close();
})();
"
EOF

echo "📥 Downloading..."
scp -o StrictHostKeyChecking=no -P $SSH_PORT root@$SSH_HOST:/tmp/vast_screenshot.png ~/clawd/Mittens/screenshots/$OUTPUT

echo "✅ Saved: ~/clawd/Mittens/screenshots/$OUTPUT"
