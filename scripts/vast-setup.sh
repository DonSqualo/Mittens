#!/bin/bash
# First-time Vast.ai instance setup for Mittens screenshots
# Only sets up renderer + Puppeteer (server runs on your machine)
#
# Usage: ssh into instance, then:
#   curl -sSL https://raw.githubusercontent.com/DonSqualo/Mittens/Lymph/scripts/vast-setup.sh | bash

set -e

echo "🚀 Setting up Mittens screenshot environment..."

apt-get update
apt-get install -y git curl xvfb

# Node.js 20
if ! command -v node &> /dev/null; then
  echo "📦 Installing Node.js..."
  curl -fsSL https://deb.nodesource.com/setup_20.x | bash -
  apt-get install -y nodejs
fi

# Clone Mittens
if [ ! -d "/root/Mittens" ]; then
  echo "📥 Cloning Mittens..."
  git clone https://github.com/DonSqualo/Mittens.git /root/Mittens
fi

cd /root/Mittens
git checkout Lymph
git pull origin Lymph

# Renderer deps
echo "📦 Installing renderer dependencies..."
cd /root/Mittens/renderer
npm install

# Puppeteer (downloads bundled Chrome)
echo "🖼️ Installing Puppeteer..."
cd /root
npm install puppeteer

# Virtual display
echo "🖥️ Starting Xvfb..."
pkill Xvfb 2>/dev/null || true
Xvfb :99 -screen 0 1920x1080x24 &
echo "export DISPLAY=:99" >> ~/.bashrc

# Start renderer
echo "▶️ Starting renderer..."
cd /root/Mittens/renderer
nohup npm run dev > /tmp/renderer.log 2>&1 &

echo ""
echo "✅ Setup complete!"
echo ""
echo "Renderer connects to ws://157.90.174.124:3001/ws (your server)"
echo "Take screenshots with: ~/clawd/Mittens/scripts/vast-screenshot.sh"
