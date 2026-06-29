# Hermes Hybrid 架构文档

## 概述

Hermes Hybrid 采用混合架构设计，结合 Rust 的高性能和 Python 的生态优势。

## 设计原则

### 1. 关注点分离

- **Rust Gateway**: 专注 I/O 密集型任务
  - 网络通信（WebSocket、HTTP）
  - 消息路由
  - 会话管理
  - 平台适配

- **Python Agent**: 专注计算密集型任务
  - LLM 推理
  - 工具调用
  - 上下文管理
  - Memory 管理

### 2. 进程隔离

Gateway 和 Agent 运行在不同进程中，通过 stdin/stdout 通信：

- **优势**:
  - Agent 崩溃不影响 Gateway
  - Gateway 可以重启 Agent
  - 便于独立升级和调试
  
- **劣势**:
  - 增加了通信开销（~1-5ms）
  - 需要序列化/反序列化

### 3. 协议驱动

JSON-RPC 2.0 协议定义了 Gateway 和 Agent 之间的接口：

- 标准化、可扩展
- 支持请求/响应和通知两种模式
- 便于跨语言实现

---

## 系统架构

```
┌─────────────────────────────────────────────────────────┐
│                     用户界面                              │
│  QQ · Telegram · Discord · Slack · WhatsApp · ...       │
└────────────────────┬────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────┐
│                Rust Gateway                              │
│  ┌───────────────────────────────────────────────────┐  │
│  │  Platform Adapters (QQBot, Telegram, etc.)       │  │
│  └──────────────────┬────────────────────────────────┘  │
│                     │                                    │
│  ┌──────────────────┴────────────────────────────────┐  │
│  │  Message Router                                   │  │
│  │  - Session management                             │  │
│  │  - Stream management                              │  │
│  │  - Media cache                                    │  │
│  └──────────────────┬────────────────────────────────┘  │
│                     │                                    │
│  ┌──────────────────┴────────────────────────────────┐  │
│  │  Agent Bridge                                     │  │
│  │  - Subprocess management                          │  │
│  │  - JSON-RPC request/response routing              │  │
│  │  - Notification broadcasting                      │  │
│  └──────────────────┬────────────────────────────────┘  │
└─────────────────────┼────────────────────────────────────┘
                      │
            stdin/stdout (JSON-RPC 2.0)
                      │
┌─────────────────────┼────────────────────────────────────┐
│                Python Agent                               │
│  ┌──────────────────┴────────────────────────────────┐  │
│  │  Agent Bridge Server (agent_bridge.py)            │  │
│  │  - JSON-RPC server                                │  │
│  │  - Session management                             │  │
│  └──────────────────┬────────────────────────────────┘  │
│                     │                                    │
│  ┌──────────────────┴────────────────────────────────┐  │
│  │  AIAgent                                          │  │
│  │  - Agent loop                                     │  │
│  │  - LLM providers (Anthropic, OpenAI, etc.)       │  │
│  │  - Streaming callbacks                            │  │
│  └──────────────────┬────────────────────────────────┘  │
│                     │                                    │
│  ┌──────────────────┴────────────────────────────────┐  │
│  │  Tool Registry                                    │  │
│  │  - 30+ tool backends                              │  │
│  │  - Terminal, web, file, code execution, etc.     │  │
│  └──────────────────┬────────────────────────────────┘  │
│                     │                                    │
│  ┌──────────────────┴────────────────────────────────┐  │
│  │  Memory & Skills                                  │  │
│  │  - Memory management                              │  │
│  │  - Skills orchestrator                            │  │
│  │  - Context files                                  │  │
│  └───────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
```

---

## 数据流

### 1. 用户消息接收

```
用户发送消息
    ↓
Platform Adapter 接收 (gateway)
    ↓
Message Router 路由
    ↓
Agent Bridge 封装为 JSON-RPC
    ↓
Python Agent 接收 (stdin)
    ↓
Agent Bridge Server 解析
    ↓
AIAgent 处理
```

### 2. Agent 响应流式输出

```
AIAgent 生成 token
    ↓
发送 stream_chunk 通知 (stdout)
    ↓
Agent Bridge 接收
    ↓
Notification 广播到订阅者
    ↓
Message Router 路由
    ↓
Platform Adapter 发送给用户
```

### 3. 工具调用

```
AIAgent 调用工具
    ↓
发送 tool_started 通知
    ↓
Agent Bridge 接收
    ↓
Platform Adapter 显示进度
    ↓
工具执行完成
    ↓
发送 tool_completed 通知
    ↓
Platform Adapter 更新进度卡片
```

---

## 组件详解

### Gateway 组件 (Rust)

#### 1. Platform Adapters

每个平台一个适配器，实现 `PlatformAdapter` trait：

```rust
#[async_trait]
pub trait PlatformAdapter: Send + Sync {
    async fn start(&self) -> Result<(), GatewayError>;
    async fn stop(&self) -> Result<(), GatewayError>;
    async fn send_message(&self, chat_id: &str, text: String) -> Result<(), GatewayError>;
    async fn send_typing(&self, chat_id: &str) -> Result<(), GatewayError>;
    fn platform_name(&self) -> &str;
}
```

**特殊功能** (QQBot):
- C2C 流式协议 (`send_c2c_stream_chunk`)
- Progress card 合并 (`send_progress_card`)
- 流式完成通知 (`send_stream_end_notice`)
- 元数据脚注 (`format_metadata_footer`)

#### 2. Message Router

职责：
- 路由入站消息到正确的 Agent session
- 管理出站消息队列
- 处理消息重试和错误恢复

#### 3. Agent Bridge

职责：
- 启动/停止 Python Agent 子进程
- 发送 JSON-RPC 请求，等待响应
- 接收 JSON-RPC 通知，广播给订阅者
- 心跳检测，自动重启

核心数据结构：
```rust
pub struct AgentBridge {
    subprocess: Arc<RwLock<Option<AgentSubprocess>>>,
    pending_requests: Arc<RwLock<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>>,
    notification_subscribers: Arc<RwLock<HashMap<String, mpsc::UnboundedSender<AgentNotification>>>>,
    next_id: Arc<RwLock<u64>>,
    config: BridgeConfig,
}
```

### Agent 组件 (Python)

#### 1. Agent Bridge Server

职责：
- 读取 stdin 的 JSON-RPC 请求
- 解析并路由到对应方法
- 发送 JSON-RPC 响应/通知到 stdout

核心方法：
- `start_session`: 初始化 AIAgent
- `handle_message`: 运行 agent loop，发送流式通知
- `interrupt`: 中断执行
- `end_session`: 清理 session
- `ping`: 心跳响应

#### 2. AIAgent (修改)

新增流式回调支持：

```python
async def run_stream(self, user_message: str):
    """Run agent loop with streaming callbacks."""
    
    async for chunk in self.llm_provider.stream_completion(...):
        yield StreamEvent(type="text", content=chunk)
    
    for tool_call in tool_calls:
        yield StreamEvent(type="tool_start", tool_name=tool_call.name, ...)
        result = await self.execute_tool(tool_call)
        yield StreamEvent(type="tool_end", tool_name=tool_call.name, ...)
```

#### 3. Tool Registry

无需修改，保持原有实现。

---

## 通信协议

### JSON-RPC 2.0

**请求格式**:
```json
{
  "jsonrpc": "2.0",
  "method": "method_name",
  "params": { ... },
  "id": 1
}
```

**响应格式**:
```json
{
  "jsonrpc": "2.0",
  "result": { ... },
  "id": 1
}
```

**通知格式** (无 id):
```json
{
  "jsonrpc": "2.0",
  "method": "notification_name",
  "params": { ... }
}
```

### 传输层

- **Line-delimited JSON**: 每条消息一行，以 `\n` 结尾
- **Buffering**: Gateway 使用 `BufReader::lines()`，Agent 使用 `sys.stdout.flush()`
- **Encoding**: UTF-8

### 错误处理

标准 JSON-RPC 错误码：
- `-32700`: Parse error
- `-32600`: Invalid request
- `-32601`: Method not found
- `-32602`: Invalid params
- `-32603`: Internal error

应用层错误码：
- `-40001`: Session not found
- `-40002`: Tool execution failed
- `-40003`: LLM API error
- `-40004`: Rate limited
- `-40005`: Context too long

详见 [protocol.md](protocol.md)。

---

## 部署拓扑

### 单机部署 (推荐)

```
┌─────────────────────────────┐
│  Linux Server / ARM Device  │
│                             │
│  ┌─────────────────────┐   │
│  │  Rust Gateway       │   │
│  │  (systemd service)  │   │
│  └──────────┬──────────┘   │
│             │ fork/exec    │
│  ┌──────────┴──────────┐   │
│  │  Python Agent       │   │
│  │  (subprocess)       │   │
│  └─────────────────────┘   │
└─────────────────────────────┘
```

优势：
- 低延迟（进程间通信）
- 简单部署
- 资源共享

### 分布式部署 (未来)

```
┌─────────────────────┐
│  Gateway Cluster    │
│  (Rust)             │
└──────────┬──────────┘
           │ HTTP/gRPC
┌──────────┴──────────┐
│  Agent Pool         │
│  (Python)           │
└─────────────────────┘
```

需要修改：
- Agent Bridge 支持 HTTP/gRPC
- 负载均衡
- Session 亲和性

---

## 性能分析

### 延迟分解

```
用户消息 → Platform Adapter → Message Router → Agent Bridge
  (网络)      (~1ms)              (~0.5ms)         (~1ms)
                                                       ↓
                                            JSON-RPC encode (~0.5ms)
                                                       ↓
                                            stdin write (~0.5ms)
                                                       ↓
Agent Bridge Server ← JSON-RPC decode ← stdin read
      (~0.5ms)              (~0.5ms)       (~1ms)
          ↓
       AIAgent (~100-2000ms, 取决于 LLM)
          ↓
通知返回路径 (~4ms)
```

**总延迟**: ~10ms (不含 LLM)

### 内存占用

| 组件 | 内存占用 |
|------|---------|
| Rust Gateway | ~30MB |
| Python Agent (空闲) | ~100MB |
| Python Agent (活跃, 1 session) | ~150MB |
| Python Agent (活跃, 5 sessions) | ~200MB |

### 吞吐量

单实例：
- **消息处理**: ~100 msg/s (受限于 Python Agent)
- **并发 session**: ~10-20 (受限于内存)

集群（分布式部署）：
- **消息处理**: 水平扩展
- **并发 session**: 受限于 Agent Pool 大小

---

## 可扩展性

### 添加新平台

1. 在 `gateway/src/platforms/` 实现 `PlatformAdapter`
2. 注册到 `main.rs`
3. 无需修改 Agent

### 添加新工具

1. 在 `agent/tools/` 实现工具
2. 注册到 Tool Registry
3. 无需修改 Gateway

### 替换 Agent 引擎

只要实现 `agent_bridge.py` 的协议接口，可以用任何语言/框架替换 Agent：

- Node.js Agent
- Go Agent
- 另一个 Rust Agent（但为何不直接用 hermes-agent-rs？）

---

## 故障恢复

### Agent 崩溃

Gateway 检测到 Agent subprocess 退出：

1. 记录错误日志
2. 等待 10 秒
3. 重启 Agent
4. 恢复活跃 sessions（如果配置了持久化）

### Gateway 崩溃

systemd 自动重启：

```ini
[Service]
Restart=on-failure
RestartSec=10s
```

### 网络断连

Platform Adapter 负责重连逻辑：

- WebSocket: 指数退避重连
- HTTP: 请求级重试

---

## 安全性

### 进程隔离

Agent 运行在独立进程，无法直接访问 Gateway 内存。

### 配置隔离

- Gateway config: `gateway/config.yaml`
- Agent config: `agent/.env`

避免 secrets 暴露到错误的进程。

### 输入验证

- Gateway: 验证平台消息格式
- Agent: 验证 JSON-RPC 请求参数

### 日志脱敏

自动移除日志中的：
- API keys
- User tokens
- 敏感消息内容

---

## 监控

### 指标

Gateway 暴露 Prometheus 指标：

```
hermes_gateway_messages_total
hermes_gateway_message_duration_seconds
hermes_gateway_agent_bridge_requests_total
hermes_gateway_agent_bridge_request_duration_seconds
hermes_gateway_agent_subprocess_restarts_total
```

### 日志

- Gateway: structured logging (tracing)
- Agent: Python logging
- 统一输出到 journald

### Healthcheck

```bash
curl http://localhost:8080/health
```

返回：
```json
{
  "status": "ok",
  "gateway": "running",
  "agent": "running",
  "uptime_secs": 12345,
  "active_sessions": 3
}
```

---

## 未来优化

### 1. 零拷贝消息传递

当前：stdin/stdout 需要序列化/反序列化

优化：共享内存 + 信号量

### 2. Agent Pool

当前：一个 Agent subprocess

优化：多个 Agent workers，负载均衡

### 3. 本地 LLM 缓存

在 Gateway 层缓存 LLM 响应，减少重复调用。

### 4. 分布式部署

Gateway 集群 + Agent 集群，支持更大规模。

---

## 参考

- [JSON-RPC 2.0 规范](https://www.jsonrpc.org/specification)
- [tokio::process](https://docs.rs/tokio/latest/tokio/process/)
- [原版 Hermes Agent](https://github.com/NousResearch/hermes-agent)
