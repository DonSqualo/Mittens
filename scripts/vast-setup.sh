#!/bin/bash
# Vast.ai instance setup for Mittens development
# Run this on a fresh instance to get everything working
#
# Usage: curl -sSL <raw-url> | bash
# Or:    scp this to instance and run it

set -e

echo "🚀 Setting up Mittens development environment..."

# Update and install essentials
apt-get update
apt-get install -y \
    build-essential \
    cmake \
    git \
    curl \
    pkg-config \
    libssl-dev \
    libclang-dev \
    llvm-dev \
    python3 \
    python3-pip \
    ffmpeg \
    xvfb

# Install Node.js if not present (nodejs 20+ includes npm)
if ! command -v node &> /dev/null; then
    curl -fsSL https://deb.nodesource.com/setup_20.x | bash -
    apt-get install -y nodejs
fi

# Install Rust
if ! command -v rustc &> /dev/null; then
    echo "📦 Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source ~/.cargo/env
fi
source ~/.cargo/env

# Install Whisper (for transcription)
echo "🎤 Installing Whisper..."
pip3 install openai-whisper

# Clone or update Mittens
if [ -d "/root/Mittens" ]; then
    echo "📥 Updating Mittens..."
    cd /root/Mittens && git pull
else
    echo "📥 Cloning Mittens..."
    git clone https://github.com/DonSqualo/Mittens.git /root/Mittens
    cd /root/Mittens
    git checkout Lymph  # or whatever branch
fi

# Build Mittens server
echo "🔨 Building Mittens server..."
cd /root/Mittens/server
cargo build --release

# Find and set up library path
LIB_DIR=$(find /root/Mittens/server/target/release/build -name "libmanifoldc.so" -path "*/out/lib/*" 2>/dev/null | head -1 | xargs dirname)
if [ -n "$LIB_DIR" ]; then
    mkdir -p /root/Mittens/lib
    cp "$LIB_DIR"/*.so* /root/Mittens/lib/
    echo "export LD_LIBRARY_PATH=/root/Mittens/lib" >> ~/.bashrc
fi

# Install renderer dependencies
echo "📦 Installing renderer dependencies..."
cd /root/Mittens/renderer
npm install

# Install puppeteer for screenshots
echo "🖼️ Installing Puppeteer..."
cd /root
npm install puppeteer

# Set up virtual display
echo "🖥️ Setting up virtual display..."
Xvfb :99 -screen 0 1920x1080x24 &
echo "export DISPLAY=:99" >> ~/.bashrc

# Create startup script
cat > /root/start-mittens.sh << 'STARTUP'
#!/bin/bash
# Start Mittens services
# Usage: ./start-mittens.sh [lua_file]

LUA_FILE="${1:-project/lymph_bath.lua}"
export LD_LIBRARY_PATH=/root/Mittens/lib
export DISPLAY=:99

# Start Xvfb if not running
pgrep Xvfb || Xvfb :99 -screen 0 1920x1080x24 &

# Kill existing services
pkill -f scriptcad-server 2>/dev/null || true
pkill -f "vite" 2>/dev/null || true
sleep 1

# Start server
cd /root/Mittens/server
nohup ./target/release/scriptcad-server ../$LUA_FILE > /tmp/server.log 2>&1 &
echo "Server started with $LUA_FILE (PID: $!)"

# Start renderer
cd /root/Mittens/renderer
nohup npm run dev > /tmp/renderer.log 2>&1 &
echo "Renderer started (PID: $!)"

sleep 3
echo "Services ready. Logs: /tmp/server.log, /tmp/renderer.log"
STARTUP
chmod +x /root/start-mittens.sh

echo ""
echo "✅ Setup complete!"
echo ""
echo "Usage:"
echo "  ./start-mittens.sh [lua_file]     # Start Mittens with optional Lua file"
echo "  whisper audio.ogg --model base    # Transcribe audio"
echo ""
echo "Library path: /root/Mittens/lib"
echo "Default Lua:  project/lymph_bath.lua"
