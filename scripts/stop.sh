#!/bin/bash
# Hermes Hybrid 停止脚本

set -e

echo "🛑 Stopping Hermes Hybrid..."

# 查找并停止 gateway 进程
GATEWAY_PIDS=$(pgrep -f "hermes-gateway" || true)

if [ -z "$GATEWAY_PIDS" ]; then
    echo "✅ No gateway process found."
else
    for PID in $GATEWAY_PIDS; do
        echo "   Stopping gateway (PID: $PID)..."
        kill -TERM "$PID" 2>/dev/null || true
    done

    # 等待进程退出
    sleep 2

    # 强制杀死还在运行的进程
    for PID in $GATEWAY_PIDS; do
        if kill -0 "$PID" 2>/dev/null; then
            echo "   Force killing gateway (PID: $PID)..."
            kill -KILL "$PID" 2>/dev/null || true
        fi
    done

    echo "✅ Gateway stopped."
fi

# Python agent 会被 gateway 自动停止，但以防万一也检查一下
AGENT_PIDS=$(pgrep -f "hermes_cli.agent_bridge" || true)

if [ -n "$AGENT_PIDS" ]; then
    for PID in $AGENT_PIDS; do
        echo "   Stopping orphan agent (PID: $PID)..."
        kill -TERM "$PID" 2>/dev/null || true
    done
fi

echo "✅ Hermes Hybrid stopped."
