#!/bin/bash
# Hermes Hybrid 启动脚本

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

echo "🚀 Starting Hermes Hybrid..."

# 检查 Rust gateway 是否已编译
if [ ! -f "$PROJECT_ROOT/gateway/target/release/hermes-gateway" ]; then
    echo "❌ Gateway binary not found. Please build first:"
    echo "   cd gateway && cargo build --release"
    exit 1
fi

# 检查 Python agent 依赖
if ! python3 -c "import hermes_cli" 2>/dev/null; then
    echo "❌ Python agent not installed. Please install first:"
    echo "   cd agent && pip install -r requirements.txt"
    exit 1
fi

# 检查配置文件
if [ ! -f "$PROJECT_ROOT/gateway/config.yaml" ]; then
    echo "❌ Config file not found: gateway/config.yaml"
    echo "   Please copy config.example.yaml and edit it."
    exit 1
fi

# 启动 gateway
echo "✅ Starting gateway..."
cd "$PROJECT_ROOT/gateway"
RUST_LOG=info ./target/release/hermes-gateway &
GATEWAY_PID=$!

echo "✅ Gateway started (PID: $GATEWAY_PID)"
echo ""
echo "📊 Logs: journalctl -f -u hermes-hybrid"
echo "🛑 Stop: ./scripts/stop.sh"
echo ""
echo "Gateway is running. Press Ctrl+C to stop."

# 等待信号
trap "kill $GATEWAY_PID 2>/dev/null; echo ''; echo '🛑 Stopped.'; exit 0" SIGINT SIGTERM

wait $GATEWAY_PID
