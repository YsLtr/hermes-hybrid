# Agent Bridge 实现进展报告

**日期**: 2026-06-29  
**状态**: Phase B & C 完成 - 桥接协议设计 + Rust 实现完成

---

## 已完成工作

### 1. 桥接协议设计 ✅

**文档位置**: `docs/agent_bridge_protocol.md`

**核心设计**:
- **传输层**: JSON-RPC 2.0 over stdin/stdout (line-delimited)
- **消息类型**:
  - Gateway → Agent: `start_session`, `handle_message`, `interrupt`, `end_session`, `ping`
  - Agent → Gateway: `typing_start`, `stream_chunk`, `tool_started`, `tool_completed`, `message_complete`, `error`

**关键特性**:
- 请求/响应模式（带 id）
- 通知模式（无 id，不期望响应）
- 超时机制（默认 300s）
- 心跳检测（默认 30s）
- 优雅关闭（SIGTERM → SIGKILL）

---

### 2. Rust Gateway 实现 ✅

**模块结构**:
```
crates/hermes-gateway/src/agent_bridge/
├── mod.rs           # AgentBridge 主结构体
├── protocol.rs      # JSON-RPC 协议定义
├── types.rs         # 类型定义（Session, Message, Notification）
└── subprocess.rs    # Python 子进程管理
```

**核心组件**:

#### `AgentBridge` (mod.rs)
- 管理 Python agent 子进程生命周期
- 维护请求/响应映射（request_id → oneshot channel）
- 广播通知到订阅者（session_id → notification channel）
- 提供高层 API：`start_session()`, `handle_message()`, `interrupt()`, `end_session()`, `ping()`

#### `AgentSubprocess` (subprocess.rs)
- 使用 `tokio::process::Command` 启动 Python agent
- stdin 使用 `Mutex<ChildStdin>` 实现 interior mutability
- stdout 通过 `BufReader` 逐行读取
- 支持优雅关闭（SIGTERM + 2s timeout → SIGKILL）

#### 通知类型 (types.rs)
```rust
pub enum AgentNotification {
    TypingStart { session_id, chat_id },
    StreamChunk { session_id, chat_id, text, is_final },
    ToolStarted { session_id, chat_id, tool_name, tool_params },
    ToolCompleted { session_id, chat_id, tool_name, success, duration_ms, result_preview },
    MessageComplete { session_id, chat_id, text, metadata },
    Error { session_id, chat_id, error_type, message, retry_after_secs },
}
```

---

## 编译状态

✅ **hermes-gateway 编译通过**

```bash
$ cargo check -p hermes-gateway
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.22s
```

---

## 下一步计划

### Phase 3: Python Agent 桥接适配器 (1-2天)

**目标**: 在 Python Hermes 中实现 `hermes_cli/agent_bridge.py`，接收 Rust gateway 的请求。

**需要实现的模块**:
```python
# hermes_cli/agent_bridge.py

import asyncio
import json
import sys
from typing import Dict, Optional

from agent.ai_agent import AIAgent
from gateway.session import SessionManager

class AgentBridgeServer:
    """
    JSON-RPC server for agent bridge protocol.
    
    Reads requests from stdin, sends notifications to stdout.
    """
    
    def __init__(self):
        self.sessions: Dict[str, AIAgent] = {}
        self.session_manager = SessionManager()
        
    async def run(self):
        """Main event loop - read stdin line by line."""
        while True:
            line = sys.stdin.readline()
            if not line:
                break
            await self.handle_request(line.strip())
    
    async def handle_request(self, line: str):
        """Parse and dispatch JSON-RPC request."""
        try:
            req = json.loads(line)
            method = req.get("method")
            params = req.get("params", {})
            req_id = req.get("id")
            
            if method == "start_session":
                result = await self.start_session(**params)
            elif method == "handle_message":
                result = await self.handle_message(**params)
            elif method == "interrupt":
                result = await self.interrupt(**params)
            elif method == "end_session":
                result = await self.end_session(**params)
            elif method == "ping":
                result = {"status": "alive", "sessions": len(self.sessions)}
            else:
                raise Exception(f"Unknown method: {method}")
            
            # Send response
            self.send_response(req_id, result)
            
        except Exception as e:
            self.send_error(req_id, -32603, str(e))
    
    def send_response(self, req_id, result):
        """Send JSON-RPC response to stdout."""
        resp = {
            "jsonrpc": "2.0",
            "result": result,
            "id": req_id
        }
        print(json.dumps(resp), flush=True)
    
    def send_notification(self, method, params):
        """Send JSON-RPC notification to stdout."""
        notif = {
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        }
        print(json.dumps(notif), flush=True)
    
    async def start_session(self, session_id, platform, chat_id, user_id, config):
        """Initialize agent session."""
        agent = AIAgent(
            model=config.get("model"),
            max_turns=config.get("max_turns", 90),
            toolsets=config.get("toolsets", ["core"]),
        )
        self.sessions[session_id] = agent
        
        return {
            "status": "ready",
            "session_id": session_id,
            "loaded_tools": len(agent.tools),
            "memory_snapshots": 2  # TODO: actual count
        }
    
    async def handle_message(self, session_id, text, attachments, reply_to_message_id):
        """Handle inbound user message - runs agent loop with streaming."""
        agent = self.sessions.get(session_id)
        if not agent:
            raise Exception(f"Session not found: {session_id}")
        
        # Send typing indicator
        self.send_notification("typing_start", {
            "session_id": session_id,
            "chat_id": session_id  # TODO: proper chat_id
        })
        
        # Run agent loop with streaming callbacks
        async for chunk in agent.run_stream(text):
            if chunk.type == "text":
                self.send_notification("stream_chunk", {
                    "session_id": session_id,
                    "chat_id": session_id,
                    "text": chunk.content,
                    "is_final": False
                })
            elif chunk.type == "tool_start":
                self.send_notification("tool_started", {
                    "session_id": session_id,
                    "chat_id": session_id,
                    "tool_name": chunk.tool_name,
                    "tool_params": chunk.tool_params
                })
            elif chunk.type == "tool_end":
                self.send_notification("tool_completed", {
                    "session_id": session_id,
                    "chat_id": session_id,
                    "tool_name": chunk.tool_name,
                    "success": chunk.success,
                    "duration_ms": chunk.duration_ms,
                    "result_preview": chunk.result_preview
                })
        
        # Send final message
        self.send_notification("message_complete", {
            "session_id": session_id,
            "chat_id": session_id,
            "text": agent.last_response,
            "metadata": {
                "model": agent.model,
                "provider": agent.provider,
                "ttft_ms": agent.ttft_ms,
                "total_time_ms": agent.total_time_ms,
                "tool_count": agent.tool_count,
                "tokens": {
                    "input": agent.input_tokens,
                    "output": agent.output_tokens
                }
            }
        })
        
        return {"status": "processing", "message_id": "msg_internal_12345"}

if __name__ == "__main__":
    server = AgentBridgeServer()
    asyncio.run(server.run())
```

**集成点**:
1. 修改原版 `AIAgent.run()` 使其支持流式回调
2. 在工具执行时触发 `tool_started` / `tool_completed` 回调
3. 确保所有输出通过 `stdout` 发送（不污染 JSON-RPC 协议）

---

### Phase 4: QQBot Adapter 增强 (2-3天)

**目标**: 在 Rust gateway 中实现 Armbian 上已有的 QQBot 增强功能。

**需要实现的功能**:

#### 4.1 C2C 流式协议
```rust
// crates/hermes-gateway/src/platforms/qqbot.rs

pub struct QqBotAdapter {
    // ... existing fields
    
    /// C2C stream state (per chat_id)
    c2c_stream_state: Arc<RwLock<HashMap<String, C2CStreamState>>>,
}

struct C2CStreamState {
    id: Option<String>,  // Message ID from QQ
    index: u32,          // Chunk index
    active: bool,        // Stream is active
}

impl QqBotAdapter {
    pub async fn send_c2c_stream_chunk(
        &self,
        openid: &str,
        content: String,
        reply_to: Option<String>,
        final_chunk: bool,
    ) -> Result<(), GatewayError> {
        let mut state = self.c2c_stream_state.write().await;
        let stream_state = state.entry(openid.to_string())
            .or_insert(C2CStreamState {
                id: None,
                index: 0,
                active: false,
            });
        
        let payload = json!({
            "content": content,
            "msg_type": if self.markdown_support { 2 } else { 0 },
            "stream": {
                "state": if final_chunk { 10 } else { 1 },
                "index": stream_state.index,
                "id": stream_state.id,
                "reset": false
            }
        });
        
        let resp = self.api_request("POST", &format!("/v2/users/{}/messages", openid), payload).await?;
        
        if final_chunk {
            stream_state.id = None;
            stream_state.index = 0;
            stream_state.active = false;
        } else {
            stream_state.id = Some(resp["id"].as_str().unwrap().to_string());
            stream_state.index += 1;
            stream_state.active = true;
        }
        
        Ok(())
    }
}
```

#### 4.2 Progress Card 合并
```rust
pub struct ProgressCardManager {
    /// Buffer of progress lines per chat
    progress_buffer: HashMap<String, Vec<String>>,
    
    /// Number of progress cards sent this turn
    progress_sent_count: HashMap<String, u32>,
    
    /// Last flush time
    last_flush: HashMap<String, Instant>,
    
    /// Config
    max_progress_messages: u32,  // Default: 2
    throttle_interval: Duration,  // Default: 3s
}

impl ProgressCardManager {
    pub fn add_progress_line(&mut self, chat_id: &str, line: String) {
        self.progress_buffer.entry(chat_id.to_string())
            .or_insert_with(Vec::new)
            .push(line);
    }
    
    pub fn should_flush(&self, chat_id: &str) -> bool {
        let sent = self.progress_sent_count.get(chat_id).unwrap_or(&0);
        if *sent >= self.max_progress_messages {
            return false;
        }
        
        let last = self.last_flush.get(chat_id);
        match last {
            None => true,
            Some(t) => t.elapsed() >= self.throttle_interval
        }
    }
    
    pub fn build_card(&self, chat_id: &str) -> Option<String> {
        let lines = self.progress_buffer.get(chat_id)?;
        if lines.is_empty() {
            return None;
        }
        
        let header = "**执行进度**\n\n";
        let body = lines.iter()
            .rev()
            .take(8)
            .rev()
            .map(|l| l.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        
        Some(format!("{}```\n{}\n```", header, body))
    }
}
```

#### 4.3 流式完成通知
```rust
pub struct StreamEndNotifier {
    /// Last notification time per chat
    last_notify: HashMap<String, Instant>,
    
    /// Notification count this turn
    notify_count: HashMap<String, u32>,
    
    /// Config
    min_interval: Duration,  // 3s
    max_per_turn: u32,      // 3
    enabled: bool,
}

impl StreamEndNotifier {
    pub async fn notify_stream_end(
        &mut self,
        adapter: &QqBotAdapter,
        chat_id: &str,
        metadata: &MessageMetadata,
    ) -> Result<(), GatewayError> {
        if !self.enabled {
            return Ok(());
        }
        
        let count = self.notify_count.entry(chat_id.to_string()).or_insert(0);
        if *count >= self.max_per_turn {
            return Ok(());
        }
        
        if let Some(last) = self.last_notify.get(chat_id) {
            if last.elapsed() < self.min_interval {
                return Ok(());
            }
        }
        
        let notice = format!(
            "✅ 已完成 ({}, {:.1}s)",
            metadata.model,
            metadata.total_time_ms as f64 / 1000.0
        );
        
        adapter.send(chat_id, notice, None).await?;
        
        *count += 1;
        self.last_notify.insert(chat_id.to_string(), Instant::now());
        
        Ok(())
    }
}
```

#### 4.4 元数据脚注
```rust
impl QqBotAdapter {
    fn format_metadata_footer(&self, metadata: &MessageMetadata) -> String {
        format!(
            "\n\n---\n_{}·{}·TTFT {}ms·总 {}ms·{}工具·{}tokens_",
            metadata.model,
            metadata.provider,
            metadata.ttft_ms.unwrap_or(0),
            metadata.total_time_ms,
            metadata.tool_count,
            metadata.tokens.input + metadata.tokens.output
        )
    }
}
```

---

### Phase 5: Gateway 集成 (1天)

**目标**: 将 `AgentBridge` 集成到 `Gateway` 主循环。

**修改点**:

#### gateway.rs
```rust
pub struct Gateway {
    // ... existing fields
    
    /// Agent bridge
    agent_bridge: Arc<AgentBridge>,
}

impl Gateway {
    pub async fn start(&self) -> Result<(), GatewayError> {
        // Start agent bridge
        self.agent_bridge.start().await?;
        
        // Start platform adapters
        for adapter in &self.adapters {
            adapter.start().await?;
        }
        
        Ok(())
    }
    
    pub async fn route_inbound_message(
        &self,
        platform: &str,
        chat_id: &str,
        user_id: &str,
        text: String,
    ) -> Result<(), GatewayError> {
        let session_id = format!("{}_{}", platform, chat_id);
        
        // Subscribe to agent notifications
        let mut notifications = self.agent_bridge.subscribe(session_id.clone()).await;
        
        // Send message to agent
        let response = self.agent_bridge.handle_message(
            session_id.clone(),
            text,
            vec![],
            None
        ).await?;
        
        // Process notifications in background
        let adapter = self.get_adapter(platform)?;
        tokio::spawn(async move {
            while let Some(notif) = notifications.recv().await {
                match notif {
                    AgentNotification::TypingStart { chat_id, .. } => {
                        adapter.send_typing(&chat_id).await.ok();
                    }
                    AgentNotification::StreamChunk { chat_id, text, is_final, .. } => {
                        if is_final {
                            adapter.send(&chat_id, text, None).await.ok();
                        } else {
                            // QQBot: use C2C stream
                            // Other platforms: buffer and edit
                        }
                    }
                    AgentNotification::ToolStarted { chat_id, tool_name, .. } => {
                        // Add to progress card buffer
                    }
                    AgentNotification::ToolCompleted { chat_id, tool_name, .. } => {
                        // Update progress card
                    }
                    AgentNotification::MessageComplete { chat_id, text, metadata, .. } => {
                        // Send final message with metadata footer
                    }
                    AgentNotification::Error { chat_id, message, .. } => {
                        adapter.send(&chat_id, format!("❌ {}", message), None).await.ok();
                    }
                }
            }
        });
        
        Ok(())
    }
}
```

---

## 测试计划

### 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_agent_bridge_lifecycle() {
        let config = BridgeConfig::default();
        let bridge = AgentBridge::new(config);
        
        bridge.start().await.unwrap();
        let ping = bridge.ping().await.unwrap();
        assert_eq!(ping.status, "alive");
        bridge.stop().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_session_management() {
        let bridge = AgentBridge::new(BridgeConfig::default());
        bridge.start().await.unwrap();
        
        let resp = bridge.start_session(
            "test_session".to_string(),
            "qqbot".to_string(),
            "test_chat".to_string(),
            "test_user".to_string(),
            SessionConfig {
                model: Some("claude-opus-4".to_string()),
                max_turns: Some(90),
                toolsets: vec!["core".to_string()],
                personality: None,
                budget: None,
            }
        ).await.unwrap();
        
        assert_eq!(resp.status, "ready");
        bridge.stop().await.unwrap();
    }
}
```

### 集成测试

在 Armbian 机器上：
1. 启动 Rust gateway
2. Python agent 通过 stdin/stdout 连接
3. 发送测试消息到 QQ bot
4. 验证：
   - Typing indicator 显示
   - 流式输出正常
   - 工具调用有进度反馈
   - 最终消息包含元数据脚注
   - 完成后有通知提示音

---

## 部署配置

### Armbian 机器

```yaml
# /root/.hermes-rs/config.yaml

gateway:
  agent_bridge:
    python_path: /usr/bin/python3
    agent_module: hermes_cli.agent_bridge
    working_dir: /root/.hermes/hermes-agent
    heartbeat_interval_secs: 30
    request_timeout_secs: 300
    auto_restart: true
    
platforms:
  qqbot:
    enabled: true
    extra:
      app_id: "..."
      client_secret: "..."
      c2c_streaming: true
      progress_coalesce: true
      metadata_footer: true
      max_progress_messages: 2
      notify_on_stream_end: true
```

### Systemd Service

```ini
# /etc/systemd/system/hermes-rs-gateway.service

[Unit]
Description=Hermes Rust Gateway with Python Agent
After=network.target

[Service]
Type=simple
User=root
WorkingDirectory=/root/.hermes-rs
ExecStart=/root/.local/bin/hermes-rs gateway start
Restart=on-failure
RestartSec=10s
Environment="RUST_LOG=info"
Environment="PYTHONPATH=/root/.hermes/hermes-agent"

[Install]
WantedBy=multi-user.target
```

---

## 预计时间表

| Phase | 任务 | 预计时间 | 状态 |
|-------|------|---------|------|
| 1 | 桥接协议设计 | 0.5天 | ✅ 完成 |
| 2 | Rust agent_bridge 实现 | 1天 | ✅ 完成 |
| 3 | Python agent_bridge.py | 1-2天 | ⏳ 待开始 |
| 4 | QQBot 增强功能 | 2-3天 | ⏳ 待开始 |
| 5 | Gateway 集成 | 1天 | ⏳ 待开始 |
| 6 | 测试与调试 | 1-2天 | ⏳ 待开始 |
| **总计** | | **6-9天** | |

---

## 风险与缓解

### 风险 1: Python agent 流式回调复杂度
**缓解**: 先实现非流式版本验证可行性，再逐步添加流式支持。

### 风险 2: JSON-RPC 协议调试困难
**缓解**: 添加详细日志，所有 stdin/stdout 交互记录到文件。

### 风险 3: Armbian 机器内存不足
**缓解**: 监控内存使用，必要时限制并发 session 数量。

---

## 下一步行动

**立即开始**: Phase 3 - Python agent_bridge.py 实现

1. 在 Armbian 机器上创建 `/root/.hermes/hermes-agent/hermes_cli/agent_bridge.py`
2. 实现基础 JSON-RPC 服务器
3. 集成现有 `AIAgent` 类
4. 测试 stdin/stdout 通信

**命令**:
```bash
ssh root@192.168.11.11
cd /root/.hermes/hermes-agent/hermes_cli
vim agent_bridge.py  # 开始实现
```

你准备好开始 Phase 3 了吗？
