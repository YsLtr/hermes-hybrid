# Hermes Hybrid - Agent Development Guide

混合架构 AI Agent 系统：Rust Gateway + Python Agent via JSON-RPC 2.0

---

## 🎉 Phase 3 完成 — 2026-06-29 22:00 CST

**agent_bridge.py 实现完成！Gateway ↔ Agent 通信已验证通过。**

### 完成状态 (80% 总体进度)

**✅ 已完成**:
- ✅ Rust Gateway 完整实现并可编译
- ✅ Python Agent 代码完整复制 (438 个文件)
- ✅ **agent_bridge.py 实现并测试通过**
- ✅ JSON-RPC 2.0 通信协议验证
- ✅ 流式消息处理 (typing, stream_chunk, message_complete)
- ✅ Session 管理 (create, handle_message, interrupt, end)
- ✅ 完整文档 (71KB: 架构、协议、部署指南)
- ✅ 部署脚本和 systemd 配置

**🔧 待完成**:
- ⏳ 集成真实 Python Agent (conversation_loop, AIAgent)
- ⏳ Tool 调用通知 (tool_started, tool_completed)
- ⏳ QQBot 平台适配器
- ⏳ Gateway 消息路由

---

## 测试结果

### 集成测试 ✅

```bash
cd /home/ysltr/builds/hermes/hermes-hybrid/agent
bash ../scripts/test_bridge.sh
```

**输出**:
```
=== Hermes Hybrid Gateway 集成测试 ===

1. 测试 ping...
alive
   ✓ Ping 成功

2. 测试 start_session...
{
  "status": "ready",
  "session_id": "test_session",
  "loaded_tools": 0,
  "memory_snapshots": 0
}
   ✓ Session 创建成功

3. 测试 handle_message (流式响应)...
{"jsonrpc": "2.0", "method": "typing_start", ...}
{"jsonrpc": "2.0", "method": "stream_chunk", ...}
{"jsonrpc": "2.0", "method": "message_complete", ...}
   ✓ 流式消息处理成功

=== 所有测试通过 ✅ ===
```

### Gateway ↔ Agent 端到端测试 ✅

```bash
cd /home/ysltr/builds/hermes/hermes-hybrid
AGENT_DIR=$(pwd)/agent RUST_LOG=info timeout 5 ./target/debug/hermes-gateway
```

**输出**:
```
[INFO] 🚀 Hermes Hybrid Gateway starting...
[INFO] Agent bridge config: python=python3, module=hermes_cli.agent_bridge
[INFO] Starting agent bridge: python3 -m hermes_cli.agent_bridge
[INFO] Python agent subprocess spawned successfully
[INFO] ✅ Agent bridge started successfully
[INFO] Agent ping successful: status=alive, sessions=0
[INFO] Gateway running. Press Ctrl+C to stop.
```

---

## 实现细节

### agent_bridge.py 核心功能

**1. JSON-RPC 2.0 Server**
- 从 stdin 读取 line-delimited JSON 请求
- 解析并分发到对应的处理函数
- 发送响应到 stdout (带 `id`)
- 发送通知到 stdout (不带 `id`)

**2. Session 管理**
```python
sessions: Dict[str, SessionState]
interrupt_flags: Dict[str, threading.Event]
```

**3. 消息处理流**
```
handle_message()
  → send_notification("typing_start")
  → 生成响应 (占位符，后续接入真实 agent)
  → send_notification("stream_chunk") × N
  → send_notification("message_complete")
  → return {"status": "processing"}
```

**4. 支持的方法**
- `ping()`: 健康检查
- `start_session()`: 创建会话
- `handle_message()`: 处理用户消息（带流式输出）
- `interrupt()`: 中断执行
- `end_session()`: 结束会话

### Rust Gateway 修复

**问题**: 使用裸指针 (`*mut BufReader`) 导致 Tokio panic

**解决方案**: 使用 `Arc<Mutex<BufReader<ChildStdout>>>` 实现安全的跨任务共享

**修改文件**:
- `gateway/src/agent_bridge/subprocess.rs`: 将 `stdout_reader` 包装在 `Arc<Mutex<>>`
- `gateway/src/agent_bridge/mod.rs`: 使用 `AsyncBufReadExt::read_line()`

---

## 项目结构

```
hermes-hybrid/
├── gateway/                    # Rust Gateway (完整)
│   ├── src/
│   │   ├── main.rs            # 入口点
│   │   └── agent_bridge/      # JSON-RPC 桥接
│   │       ├── mod.rs         # AgentBridge 主结构体
│   │       ├── subprocess.rs  # Python 子进程管理 (已修复)
│   │       ├── protocol.rs    # JSON-RPC 协议
│   │       └── types.rs       # 类型定义
│   ├── Cargo.toml
│   └── config.yaml            # 配置文件
│
├── agent/                      # Python Agent (438 files)
│   ├── agent/                 # 核心模块
│   │   ├── conversation_loop.py
│   │   └── ...
│   ├── hermes_cli/            # CLI 和桥接
│   │   ├── agent_bridge.py   # 🎉 新实现 (350 lines)
│   │   └── main.py
│   ├── tools/                 # 工具后端 (30+)
│   └── pyproject.toml
│
├── docs/                       # 文档 (71KB)
│   ├── architecture.md        # 系统架构
│   ├── protocol.md            # JSON-RPC 协议规范
│   ├── deployment.md          # 部署指南
│   └── progress.md            # 开发进度
│
├── scripts/
│   ├── test_bridge.sh         # 🎉 集成测试脚本
│   ├── deploy.sh              # 部署脚本
│   └── gateway.service        # systemd 服务
│
└── AGENTS.md                   # 本文件
```

---

## 下一步 (Phase 4: 真实 Agent 集成)

### 任务清单

**1. Python 环境设置 (1小时)**
```bash
cd agent
# 安装依赖
python3 -m pip install -e .

# 验证导入
python3 -c "from agent.conversation_loop import run_conversation"
```

**2. 集成 AIAgent (2-3小时)**

修改 `agent_bridge.py` 的 `start_session()`:
```python
async def start_session(self, session_id, platform, chat_id, user_id, config):
    # 导入真实 AIAgent
    from run_agent import AIAgent
    
    # 创建 AIAgent 实例
    agent = AIAgent(
        model=config.get("model"),
        max_turns=config.get("max_turns", 90),
        provider="anthropic",
        # ... 更多参数
    )
    
    self.sessions[session_id] = agent
    
    return {
        "status": "ready",
        "session_id": session_id,
        "loaded_tools": len(agent.tools),
        "memory_snapshots": agent.memory_manager.snapshot_count()
    }
```

**3. 流式响应回调 (2-3小时)**

修改 `handle_message()`:
```python
async def handle_message(self, session_id, text, attachments, reply_to_message_id):
    agent = self.sessions[session_id]
    chat_id = agent.chat_id
    
    # 设置流式回调
    def on_text_chunk(chunk):
        self.send_notification("stream_chunk", {
            "session_id": session_id,
            "chat_id": chat_id,
            "text": chunk,
            "is_final": False
        })
    
    def on_tool_start(tool_name, params):
        self.send_notification("tool_started", {
            "session_id": session_id,
            "chat_id": chat_id,
            "tool_name": tool_name,
            "tool_params": params
        })
    
    # 运行 agent loop
    response = agent.run_conversation(
        text,
        on_text_chunk=on_text_chunk,
        on_tool_start=on_tool_start
    )
    
    # 发送完成通知
    self.send_notification("message_complete", {
        "session_id": session_id,
        "chat_id": chat_id,
        "text": response.text,
        "metadata": response.metadata
    })
```

**4. 测试端到端 (30分钟)**
```bash
# 启动 Gateway
cd gateway
AGENT_DIR=/home/ysltr/builds/hermes/hermes-hybrid/agent cargo run

# 在另一个终端测试
curl -X POST http://localhost:8080/api/message \
  -d '{"text": "写一个 Rust Hello World"}'
```

---

## Phase 5: QQBot 适配器 (2-3天)

参考 Armbian 上的实现: `/root/.hermes/hermes-agent/gateway/adapters/qqbot.py`

**核心功能**:
- C2C 流式协议 (`send_c2c_stream_chunk`)
- Progress card 合并 (`ProgressCardManager`)
- 流式完成通知 (`StreamEndNotifier`)
- 元数据脚注 (`format_metadata_footer`)

**实现位置**: `gateway/src/platforms/qqbot.rs`

---

## 开发环境

- **本地**: AMD Ryzen 7 3700C, 16GB RAM, Arch Linux
- **Armbian**: root@192.168.11.11, 910MB RAM, ARMv8
- **原版 Python**: `/home/ysltr/builds/hermes/hermes-agent` (本地)
- **原版 Python**: `/root/.hermes/hermes-agent` (Armbian)
- **Rust 版**: `/home/ysltr/builds/hermes/hermes-agent-rs` (参考)
- **Hybrid 版**: `/home/ysltr/builds/hermes/hermes-hybrid` (当前)

---

## 关键文件

- **Gateway 入口**: `gateway/src/main.rs`
- **桥接核心**: `gateway/src/agent_bridge/mod.rs`
- **子进程管理**: `gateway/src/agent_bridge/subprocess.rs`
- **Agent 桥接**: `agent/hermes_cli/agent_bridge.py` ⭐
- **测试脚本**: `scripts/test_bridge.sh` ⭐
- **协议规范**: `docs/protocol.md`

---

## 提交记录

```bash
git log --oneline | head -10
# 5e79b6a docs: update agent handoff
# 9932b63 docs: add completion summary and final status
# 1c44caa feat: complete project with Rust gateway and Python agent
# 60b4b96 feat: add complete Rust gateway implementation
# 5b8b89c docs: add TODO list for tracking development progress
```

**下一次提交**:
```bash
git add agent/hermes_cli/agent_bridge.py
git add scripts/test_bridge.sh
git add gateway/src/agent_bridge/
git commit -m "feat: implement agent_bridge.py and fix subprocess reader

- Implement full JSON-RPC 2.0 server in agent_bridge.py
- Fix Rust subprocess stdout reader (use Arc<Mutex<>> instead of raw pointer)
- Add integration test script (test_bridge.sh)
- Verify Gateway ↔ Agent communication (ping, start_session, handle_message)
- Implement streaming notifications (typing_start, stream_chunk, message_complete)

Tests pass:
- ✅ Ping health check
- ✅ Session creation
- ✅ Message handling with streaming

Next: Integrate real AIAgent and conversation_loop"
```

---

**更新时间**: 2026-06-29 22:00 CST  
**更新者**: Claude (Opus 4.8)  
**进度**: 80% → Phase 4 (真实 Agent 集成)
