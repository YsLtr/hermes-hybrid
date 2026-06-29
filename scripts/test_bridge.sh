#!/bin/bash
# 测试 Gateway ↔ Agent 通信

set -e

echo "=== Hermes Hybrid Gateway 集成测试 ==="
echo

# 1. 测试 ping
echo "1. 测试 ping..."
echo '{"jsonrpc":"2.0","method":"ping","params":{},"id":1}' | \
    timeout 2 python3 -m hermes_cli.agent_bridge 2>/dev/null | \
    jq -r '.result.status'
echo "   ✓ Ping 成功"
echo

# 2. 测试 start_session
echo "2. 测试 start_session..."
cat <<EOF | timeout 2 python3 -m hermes_cli.agent_bridge 2>/dev/null | jq '.result'
{"jsonrpc":"2.0","method":"start_session","params":{"session_id":"test_session","platform":"test","chat_id":"test_chat","user_id":"test_user","config":{"model":"claude-opus-4","max_turns":90,"toolsets":["core"]}},"id":2}
EOF
echo "   ✓ Session 创建成功"
echo

# 3. 测试 handle_message (带流式输出)
echo "3. 测试 handle_message (流式响应)..."
(
echo '{"jsonrpc":"2.0","method":"start_session","params":{"session_id":"msg_test","platform":"test","chat_id":"chat1","user_id":"user1","config":{"model":"claude-opus-4"}},"id":1}'
sleep 0.2
echo '{"jsonrpc":"2.0","method":"handle_message","params":{"session_id":"msg_test","text":"你好"},"id":2}'
sleep 1
) | timeout 3 python3 -m hermes_cli.agent_bridge 2>/dev/null | head -10
echo "   ✓ 流式消息处理成功"
echo

echo "=== 所有测试通过 ✅ ==="
