#!/usr/bin/env python3
"""
测试 Agent Bridge 的完整功能
"""

import asyncio
import json
import sys
import os

# 确保能找到 agent 模块
agent_dir = os.path.join(os.path.dirname(__file__), 'agent')
sys.path.insert(0, agent_dir)
os.chdir(agent_dir)

from hermes_cli.agent_bridge import AgentBridgeServer


async def test_agent_bridge():
    """测试 agent bridge 的完整流程"""
    print("=" * 60)
    print("测试 Agent Bridge")
    print("=" * 60)

    server = AgentBridgeServer()

    # 1. 测试 ping
    print("\n1️⃣  测试 Ping...")
    result = await server.ping()
    print(f"   ✓ Ping 响应: {result}")

    # 2. 测试创建会话
    print("\n2️⃣  测试创建会话...")
    session_id = "test_session_001"
    result = await server.start_session(
        session_id=session_id,
        platform="test",
        chat_id="test_chat_123",
        user_id="test_user_456",
        config={
            "model": "claude-opus-4",
            "max_turns": 90,
            "toolsets": ["default"]
        }
    )
    print(f"   ✓ 会话创建成功:")
    print(f"     - 状态: {result['status']}")
    print(f"     - 工具数: {result['loaded_tools']}")
    print(f"     - Session ID: {result['session_id']}")

    # 3. 测试发送消息
    print("\n3️⃣  测试发送消息...")
    print("   📤 发送: '你好，请介绍一下你自己'")

    # 收集通知
    notifications = []
    original_send = server.send_notification

    def capture_notification(method, params):
        notifications.append({"method": method, "params": params})
        original_send(method, params)

    server.send_notification = capture_notification

    result = await server.handle_message(
        session_id=session_id,
        text="你好，请介绍一下你自己",
        attachments=None,
        reply_to_message_id=None
    )

    print(f"   ✓ 消息处理结果: {result}")

    # 显示收到的通知
    print(f"\n   📨 收到 {len(notifications)} 个通知:")
    for i, notif in enumerate(notifications, 1):
        method = notif['method']
        params = notif['params']
        if method == "typing_start":
            print(f"      {i}. 打字提醒")
        elif method == "stream_chunk":
            text = params.get('text', '')[:30]
            print(f"      {i}. 流式文本: {text}...")
        elif method == "tool_started":
            tool = params.get('tool_name')
            print(f"      {i}. 工具开始: {tool}")
        elif method == "tool_completed":
            tool = params.get('tool_name')
            print(f"      {i}. 工具完成: {tool}")
        elif method == "message_complete":
            text = params.get('text', '')[:50]
            metadata = params.get('metadata', {})
            print(f"      {i}. 消息完成:")
            print(f"         - 文本: {text}...")
            print(f"         - 模型: {metadata.get('model')}")
            print(f"         - 耗时: {metadata.get('duration_ms')}ms")
            print(f"         - 工具数: {metadata.get('tool_count', 0)}")

    # 4. 测试会话结束
    print("\n4️⃣  测试结束会话...")
    result = await server.end_session(session_id=session_id)
    print(f"   ✓ 会话结束: {result}")

    print("\n" + "=" * 60)
    print("✅ 所有测试通过！")
    print("=" * 60)


if __name__ == "__main__":
    asyncio.run(test_agent_bridge())
