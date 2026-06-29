#!/bin/bash
# 从原版 hermes-agent 复制文件到 agent 目录

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
AGENT_DIR="$PROJECT_ROOT/agent"

echo "📦 准备 Python Agent..."

# 检查原版 hermes-agent 路径
if [ -z "$HERMES_AGENT_PATH" ]; then
    # 尝试默认位置
    if [ -d "/root/.hermes/hermes-agent" ]; then
        HERMES_AGENT_PATH="/root/.hermes/hermes-agent"
    elif [ -d "$HOME/.hermes/hermes-agent" ]; then
        HERMES_AGENT_PATH="$HOME/.hermes/hermes-agent"
    else
        echo "❌ 找不到原版 hermes-agent"
        echo "请设置环境变量: export HERMES_AGENT_PATH=/path/to/hermes-agent"
        echo "或者使用 git submodule 方式"
        exit 1
    fi
fi

if [ ! -d "$HERMES_AGENT_PATH" ]; then
    echo "❌ 目录不存在: $HERMES_AGENT_PATH"
    exit 1
fi

echo "✅ 找到原版: $HERMES_AGENT_PATH"

# 复制核心文件
echo "📁 复制文件..."
cp -rv "$HERMES_AGENT_PATH/agent" "$AGENT_DIR/"
cp -rv "$HERMES_AGENT_PATH/tools" "$AGENT_DIR/"
cp -rv "$HERMES_AGENT_PATH/hermes_cli" "$AGENT_DIR/"
cp -v "$HERMES_AGENT_PATH/requirements.txt" "$AGENT_DIR/"

# 复制 .env.example
if [ -f "$HERMES_AGENT_PATH/.env.example" ]; then
    cp -v "$HERMES_AGENT_PATH/.env.example" "$AGENT_DIR/"
fi

# 复制 agent_bridge 模板
echo "🔧 创建 agent_bridge.py..."
cp -v "$SCRIPT_DIR/agent_bridge_template.py" "$AGENT_DIR/hermes_cli/agent_bridge.py"

echo ""
echo "✅ Agent 目录准备完成！"
echo ""
echo "下一步:"
echo "  1. 编辑 agent/.env 填写 API keys"
echo "  2. 安装依赖: cd agent && pip3 install -r requirements.txt"
echo "  3. 完善 agent_bridge.py 实现"
echo ""
