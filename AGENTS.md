# Hermes Hybrid - Agent Development Guide

混合架构 AI Agent 系统：Rust Gateway + Python Agent via JSON-RPC 2.0

---

## Active Handoff — 2026-06-29 23:50 CST

**目标**: 实现 Python agent_bridge.py，完成 Gateway ↔ Agent 通信

### 当前状态

**项目创建完成** (70% 总体进度):
- ✅ Rust Gateway 完整实现并可编译 (`cargo check` 通过)
- ✅ Python Agent 代码完整复制 (438 个文件，从 `/home/ysltr/builds/hermes/hermes-agent`)
- ✅ 完整文档 (71KB: 架构、协议、部署指南)
- ✅ 部署脚本和 systemd 配置
- ⏳ `agent/hermes_cli/agent_bridge.py` 有模板，需实现核心逻辑

**仓库统计**:
- 位置: `/home/ysltr/builds/hermes/hermes-hybrid`
- 大小: 115MB
- 提交: 6 个
- 文件: 790 个 (438 Python + 11 Rust)
- 代码行数: 324,274 行

**关键文件**:
```
gateway/src/agent_bridge/     # Rust 桥接模块 (完整)
  ├── mod.rs                  # AgentBridge 主结构体
  ├── subprocess.rs           # Python 子进程管理
  ├── protocol.rs             # JSON-RPC 协议
  └── types.rs                # 类型定义

agent/hermes_cli/agent_bridge.py  # 🔧 待实现 (当前只有模板)

gateway/config.example.yaml   # Gateway 配置示例
agent/.env.example            # API keys 模板
```

**编译状态**:
```bash
cd /home/ysltr/builds/hermes/hermes-hybrid/gateway
cargo build --release  # ✅ 通过，二进制 29MB
```

### 下一步 (Phase 3: Agent Bridge 实现)

**立即任务**: 实现 `agent/hermes_cli/agent_bridge.py`

1. **导入依赖**:
   ```python
   from agent.ai_agent import AIAgent
   from agent.conversation_loop import run_agent_loop
   # 等
   ```

2. **实现协议方法**:
   - `start_session()`: 创建 AIAgent 实例，加载工具和 memory
   - `handle_message()`: 运行 agent loop，发送流式通知
   - `ping()`: 心跳响应 (已有占位符)
   - `interrupt()`: 中断执行
   - `end_session()`: 清理 session

3. **添加流式回调**:
   ```python
   async def handle_message(self, session_id, text, ...):
       agent = self.sessions[session_id]
       
       # Typing indicator
       self.send_notification("typing_start", {...})
       
       # Run agent loop with callbacks
       async for event in agent.run_stream(text):
           if event.type == "text":
               self.send_notification("stream_chunk", {...})
           elif event.type == "tool_start":
               self.send_notification("tool_started", {...})
           elif event.type == "tool_end":
               self.send_notification("tool_completed", {...})
       
       # Final message
       self.send_notification("message_complete", {...})
   ```

4. **修改 AIAgent 支持流式** (如果需要):
   - 在 `agent/agent/conversation_loop.py` 添加 `run_stream()` 方法
   - 或通过回调钩子实现

5. **测试**:
   ```bash
   cd agent
   python3 -m hermes_cli.agent_bridge
   # 手动输入 JSON-RPC 测试:
   # {"jsonrpc":"2.0","method":"ping","params":{},"id":1}
   ```

6. **端到端集成**:
   ```bash
   # Terminal 1: 启动 Gateway
   cd gateway
   PYTHON_PATH=python3 AGENT_DIR=/home/ysltr/builds/hermes/hermes-hybrid/agent cargo run
   
   # 观察 Gateway ↔ Agent 通信日志
   ```

### 参考文档

- `docs/protocol.md`: JSON-RPC 2.0 协议完整规范
- `docs/progress.md`: agent_bridge.py 实现框架 (Python 代码示例)
- `docs/architecture.md`: 系统架构和数据流
- `scripts/agent_bridge_template.py`: 当前模板 (占位符实现)
- `COMPLETION.md`: 项目完成总结

### 已知约束

- Python agent 使用 `pyproject.toml` 管理依赖 (不是 requirements.txt)
- Gateway 通过环境变量配置 Python 路径:
  - `PYTHON_PATH`: Python 可执行文件 (默认 `python3`)
  - `AGENT_DIR`: Agent 工作目录
- stdin/stdout 通信必须 line-delimited JSON，每条消息以 `\n` 结尾
- stdout 必须 flush 才能被 Gateway 读取: `print(json.dumps(msg), flush=True)`
- stderr 用于日志，不影响协议通信

### 后续阶段

**Phase 4: QQBot 增强** (2-3天):
- C2C 流式协议 (`send_c2c_stream_chunk`)
- Progress card 合并 (`ProgressCardManager`)
- 流式完成通知 (`StreamEndNotifier`)
- 元数据脚注 (`format_metadata_footer`)

参考 Armbian 上的 Python 实现: `/root/.hermes/hermes-agent/gateway/adapters/qqbot.py`

**Phase 5: Gateway 集成** (1天):
- 在 `gateway/src/main.rs` 添加平台适配器
- 实现 `route_inbound_message()` 路由逻辑
- 处理 agent notifications 并调用平台 adapter

### 开发环境

- **本地**: AMD Ryzen 7 3700C, 16GB RAM, Arch Linux
- **Armbian**: root@192.168.11.11, 910MB RAM, ARMv8
- **原版 Python**: `/home/ysltr/builds/hermes/hermes-agent` (本地)
- **原版 Python**: `/root/.hermes/hermes-agent` (Armbian)
- **Rust 版**: `/home/ysltr/builds/hermes/hermes-agent-rs` (参考)

### 建议技能

无，直接继续实现 agent_bridge.py。

---

## 项目概览

### 架构

```
┌─────────────────────┐
│  Rust Gateway       │ stdin/stdout (JSON-RPC 2.0)
│  - Platform adapters│ ←─────────────────────────→
│  - Message routing  │
└─────────────────────┘
                       
┌─────────────────────┐
│  Python Agent       │
│  - Agent loop       │
│  - Tools (30+)      │
│  - Memory & Skills  │
└─────────────────────┘
```

### 为什么混合架构？

- **Rust Gateway**: 高性能 I/O、低内存 (30MB)
- **Python Agent**: 复用现有完整功能
- **JSON-RPC**: 标准化进程间通信
- **适合 ARM**: Armbian 910MB RAM 可运行

### 核心模块

- `gateway/src/agent_bridge/`: JSON-RPC 桥接 (Rust)
- `agent/agent/`: Agent loop 核心 (Python)
- `agent/tools/`: 工具后端 (Python)
- `agent/hermes_cli/`: CLI 和配置 (Python)

### 开发流程

1. 实现 `agent_bridge.py`
2. 测试 Gateway ↔ Agent 通信
3. 添加平台适配器 (QQBot 等)
4. 部署到 Armbian
5. 性能优化

---

**更新时间**: 2026-06-29 23:50 CST  
**更新者**: Claude (Opus 4.8)
