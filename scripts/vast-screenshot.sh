#!/bin/bash
# Take a WebGL screenshot from Vast.ai GPU instance
# Renderer connects to your server (157.90.174.124:3001) - no server needed on Vast.ai
#
# Usage: ./vast-screenshot.sh [options] [output.png]
#   --restart    Git pull + restart renderer (for renderer code changes)
#   --branch X   Switch to branch X before pull (default: Lymph)
#
# Examples:
#   ./vast-screenshot.sh                      # Quick screenshot
#   ./vast-screenshot.sh lymph_v2.png         # Custom filename
#   ./vast-screenshot.sh --restart            # Update code first

set -e

RESTART=false
BRANCH="Lymph"
OUTPUT="screenshot_$(date +%Y%m%d_%H%M%S).png"

while [[ $# -gt 0 ]]; do
  case $1 in
    --restart) RESTART=true; shift ;;
    --branch) BRANCH="$2"; shift 2 ;;
    *) OUTPUT="$1"; shift ;;
  esac
done

# Get API key
export VAST_API_KEY=$(cat ~/.config/vastai/vast_api_key 2>/dev/null)
if [ -z "$VAST_API_KEY" ]; then
  echo "❌ No API key found at ~/.config/vastai/vast_api_key"
  exit 1
fi

# Find running instance
echo "🔍 Finding running instance..."
INSTANCE_INFO=$(~/.local/bin/vastai show instances --raw 2>/dev/null | jq -r '.[] | select(.actual_status == "running") | "\(.ssh_host) \(.ssh_port) \(.id)"' | head -1)

if [ -z "$INSTANCE_INFO" ]; then
  echo "❌ No running Vast.ai instance"
  echo ""
  echo "Start one with:"
  echo "  vastai search offers 'gpu_ram>=4 reliability>0.95 dph<0.15' -o dph"
  echo "  vastai create instance <ID> --image ubuntu:22.04 --disk 20 --ssh"
  echo "  # Then run vast-setup.sh on it"
  exit 1
fi

SSH_HOST=$(echo $INSTANCE_INFO | cut -d' ' -f1)
SSH_PORT=$(echo $INSTANCE_INFO | cut -d' ' -f2)
INSTANCE_ID=$(echo $INSTANCE_INFO | cut -d' ' -f3)
SSH="ssh -o StrictHostKeyChecking=no -o ConnectTimeout=15 -p $SSH_PORT root@$SSH_HOST"

echo "📡 Instance $INSTANCE_ID @ $SSH_HOST:$SSH_PORT"

# Restart renderer if requested
if [ "$RESTART" = true ]; then
  echo "🔄 Updating renderer (branch: $BRANCH)..."
  $SSH "bash -s" << EOF
cd /root/Mittens
git fetch origin
git checkout $BRANCH
git pull origin $BRANCH

pkill -f vite 2>/dev/null || true
sleep 1

cd /root/Mittens/renderer
nohup npm run dev > /tmp/renderer.log 2>&1 &
sleep 3
echo "✓ Renderer restarted"
EOF
fi

# Take screenshot
echo "📸 Taking screenshot..."
$SSH 'bash -s' << 'EOF'
export DISPLAY=:99

# Ensure Xvfb is running
pgrep Xvfb || Xvfb :99 -screen 0 1920x1080x24 &
sleep 1

# Ensure renderer is running
if ! pgrep -f vite > /dev/null; then
  echo "Starting renderer..."
  cd /root/Mittens/renderer
  nohup npm run dev > /tmp/renderer.log 2>&1 &
  sleep 4
fi

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
  await page.screenshot({ path: '/tmp/screenshot.png' });
  await browser.close();
  console.log('Done');
})().catch(e => { console.error(e.message); process.exit(1); });
"
EOF

# Download
echo "📥 Downloading..."
mkdir -p ~/clawd/Mittens/screenshots
scp -o StrictHostKeyChecking=no -P $SSH_PORT root@$SSH_HOST:/tmp/screenshot.png ~/clawd/Mittens/screenshots/$OUTPUT

echo "✅ ~/clawd/Mittens/screenshots/$OUTPUT"
