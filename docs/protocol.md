# Agent Bridge Protocol

**Version**: 1.0  
**Transport**: JSON-RPC 2.0 over stdin/stdout

## Overview

The Rust gateway communicates with the Python agent via JSON-RPC messages over stdio pipes. The gateway spawns the Python agent as a subprocess and maintains bidirectional communication.

---

## Message Flow

```
┌─────────────┐                        ┌──────────────┐
│ Rust Gateway│                        │ Python Agent │
└──────┬──────┘                        └──────┬───────┘
       │                                      │
       │  1. start_session                    │
       ├─────────────────────────────────────>│
       │                                      │
       │  2. session_started                  │
       │<─────────────────────────────────────┤
       │                                      │
       │  3. handle_message                   │
       ├─────────────────────────────────────>│
       │                                      │
       │  4. progress_update (streaming)      │
       │<─────────────────────────────────────┤
       │                                      │
       │  5. tool_started                     │
       │<─────────────────────────────────────┤
       │                                      │
       │  6. tool_completed                   │
       │<─────────────────────────────────────┤
       │                                      │
       │  7. message_complete                 │
       │<─────────────────────────────────────┤
       │                                      │
```

---

## 1. Gateway → Agent (Requests)

### 1.1 `start_session`

Initialize a new session or resume an existing one.

```json
{
  "jsonrpc": "2.0",
  "method": "start_session",
  "params": {
    "session_id": "qqbot_87FAB80C79F56E0EFB3E5B8590AF00BC",
    "platform": "qqbot",
    "chat_id": "87FAB80C79F56E0EFB3E5B8590AF00BC",
    "user_id": "87FAB80C79F56E0EFB3E5B8590AF00BC",
    "config": {
      "model": "claude-opus-4",
      "max_turns": 90,
      "toolsets": ["core", "web", "vision"]
    }
  },
  "id": 1
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "result": {
    "status": "ready",
    "session_id": "qqbot_87FAB80C79F56E0EFB3E5B8590AF00BC",
    "loaded_tools": 30,
    "memory_snapshots": 2
  },
  "id": 1
}
```

---

### 1.2 `handle_message`

Send an inbound user message to the agent for processing.

```json
{
  "jsonrpc": "2.0",
  "method": "handle_message",
  "params": {
    "session_id": "qqbot_87FAB80C79F56E0EFB3E5B8590AF00BC",
    "text": "帮我搜索最新的 Rust 异步编程最佳实践",
    "attachments": [
      {
        "type": "image",
        "url": "https://example.com/image.jpg",
        "caption": "这是截图"
      }
    ],
    "reply_to_message_id": "msg_12345"
  },
  "id": 2
}
```

**Response (immediate acknowledgment):**
```json
{
  "jsonrpc": "2.0",
  "result": {
    "status": "processing",
    "message_id": "msg_internal_67890"
  },
  "id": 2
}
```

**Note:** The actual agent response comes via notifications (see section 2 below).

---

### 1.3 `interrupt`

Interrupt an ongoing agent execution.

```json
{
  "jsonrpc": "2.0",
  "method": "interrupt",
  "params": {
    "session_id": "qqbot_87FAB80C79F56E0EFB3E5B8590AF00BC",
    "reason": "user_cancelled"
  },
  "id": 3
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "result": {
    "status": "interrupted"
  },
  "id": 3
}
```

---

### 1.4 `end_session`

Gracefully terminate a session.

```json
{
  "jsonrpc": "2.0",
  "method": "end_session",
  "params": {
    "session_id": "qqbot_87FAB80C79F56E0EFB3E5B8590AF00BC"
  },
  "id": 4
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "result": {
    "status": "ended",
    "turns_processed": 15,
    "total_tokens": 45000
  },
  "id": 4
}
```

---

## 2. Agent → Gateway (Notifications)

These are **JSON-RPC notifications** (no `id` field, no response expected).

### 2.1 `typing_start`

Notify gateway that the agent is typing (for typing indicator).

```json
{
  "jsonrpc": "2.0",
  "method": "typing_start",
  "params": {
    "session_id": "qqbot_87FAB80C79F56E0EFB3E5B8590AF00BC",
    "chat_id": "87FAB80C79F56E0EFB3E5B8590AF00BC"
  }
}
```

---

### 2.2 `stream_chunk`

Stream a text chunk from the LLM response.

```json
{
  "jsonrpc": "2.0",
  "method": "stream_chunk",
  "params": {
    "session_id": "qqbot_87FAB80C79F56E0EFB3E5B8590AF00BC",
    "chat_id": "87FAB80C79F56E0EFB3E5B8590AF00BC",
    "text": "根据搜索结果，Rust 异步编程的",
    "is_final": false
  }
}
```

---

### 2.3 `tool_started`

Notify that a tool execution has started.

```json
{
  "jsonrpc": "2.0",
  "method": "tool_started",
  "params": {
    "session_id": "qqbot_87FAB80C79F56E0EFB3E5B8590AF00BC",
    "chat_id": "87FAB80C79F56E0EFB3E5B8590AF00BC",
    "tool_name": "web_search",
    "tool_params": {
      "query": "Rust async programming best practices 2026"
    }
  }
}
```

---

### 2.4 `tool_completed`

Notify that a tool execution has completed.

```json
{
  "jsonrpc": "2.0",
  "method": "tool_completed",
  "params": {
    "session_id": "qqbot_87FAB80C79F56E0EFB3E5B8590AF00BC",
    "chat_id": "87FAB80C79F56E0EFB3E5B8590AF00BC",
    "tool_name": "web_search",
    "success": true,
    "duration_ms": 1200,
    "result_preview": "找到 5 个相关结果..."
  }
}
```

---

### 2.5 `message_complete`

Final response for a user message (includes metadata).

```json
{
  "jsonrpc": "2.0",
  "method": "message_complete",
  "params": {
    "session_id": "qqbot_87FAB80C79F56E0EFB3E5B8590AF00BC",
    "chat_id": "87FAB80C79F56E0EFB3E5B8590AF00BC",
    "text": "根据搜索结果，Rust 异步编程的最佳实践包括...",
    "metadata": {
      "model": "claude-opus-4",
      "provider": "anthropic",
      "ttft_ms": 850,
      "total_time_ms": 3200,
      "tool_count": 2,
      "tokens": {
        "input": 1200,
        "output": 580
      }
    }
  }
}
```

---

### 2.6 `error`

Report an error during processing.

```json
{
  "jsonrpc": "2.0",
  "method": "error",
  "params": {
    "session_id": "qqbot_87FAB80C79F56E0EFB3E5B8590AF00BC",
    "chat_id": "87FAB80C79F56E0EFB3E5B8590AF00BC",
    "error_type": "rate_limited",
    "message": "API rate limit exceeded. Retrying in 30s...",
    "retry_after_secs": 30
  }
}
```

---

## 3. Platform-Specific Features

### 3.1 QQBot Progress Coalescing

For QQBot, the gateway needs to accumulate tool progress and send coalesced progress cards. The agent sends individual tool events, and the gateway buffers them:

**Agent sends:**
```json
{"jsonrpc": "2.0", "method": "tool_started", "params": {"tool_name": "web_search", ...}}
{"jsonrpc": "2.0", "method": "tool_started", "params": {"tool_name": "read_file", ...}}
{"jsonrpc": "2.0", "method": "tool_completed", "params": {"tool_name": "web_search", ...}}
```

**Gateway accumulates and sends QQ progress card every 3s:**
```markdown
**执行进度**

```
✓ web_search (1.2s)
⏳ read_file
```
```

---

### 3.2 QQBot Stream Protocol

For C2C messages, the gateway uses `send_c2c_stream_chunk()` with:
- `state=1` for intermediate chunks
- `state=10` for final chunk
- Reuses `id` from previous chunk
- Appends metadata footer to final chunk

---

## 4. Error Handling

### 4.1 JSON-RPC Errors

Standard JSON-RPC error codes:
- `-32700`: Parse error
- `-32600`: Invalid request
- `-32601`: Method not found
- `-32602`: Invalid params
- `-32603`: Internal error

### 4.2 Application Errors

Custom error codes (negative):
- `-40001`: Session not found
- `-40002`: Tool execution failed
- `-40003`: LLM API error
- `-40004`: Rate limited
- `-40005`: Context too long

**Example:**
```json
{
  "jsonrpc": "2.0",
  "error": {
    "code": -40004,
    "message": "Rate limited by Anthropic API",
    "data": {
      "retry_after_secs": 60
    }
  },
  "id": 2
}
```

---

## 5. Lifecycle Management

### 5.1 Agent Process Startup

The Rust gateway spawns the Python agent with:
```bash
python3 -m hermes_cli.agent_bridge
```

The agent module (`hermes_cli/agent_bridge.py`) handles JSON-RPC over stdin/stdout.

### 5.2 Heartbeat

Gateway sends periodic heartbeat to detect agent crashes:
```json
{
  "jsonrpc": "2.0",
  "method": "ping",
  "params": {},
  "id": 999
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "result": {
    "status": "alive",
    "sessions": 3
  },
  "id": 999
}
```

### 5.3 Graceful Shutdown

On gateway shutdown:
1. Send `shutdown` notification to agent
2. Wait up to 5s for agent to flush buffers
3. Send SIGTERM if still running
4. Send SIGKILL after 2s timeout

---

## 6. Implementation Notes

### 6.1 Line-Delimited JSON

Each JSON-RPC message is **one line** terminated by `\n`:
```
{"jsonrpc":"2.0","method":"handle_message","params":{...},"id":1}\n
{"jsonrpc":"2.0","method":"stream_chunk","params":{...}}\n
```

### 6.2 Buffering

- Gateway uses `BufReader::lines()` for reading
- Agent uses `sys.stdout.flush()` after each message
- No partial line reads

### 6.3 Concurrency

- Gateway handles multiple sessions concurrently
- Each session maintains its own JSON-RPC request/response map
- Notifications are broadcast to all relevant sessions

---

## 7. Testing

### 7.1 Mock Agent

For testing, Rust can spawn a mock agent:
```rust
// crates/hermes-gateway/tests/mock_agent.rs
#[tokio::test]
async fn test_agent_bridge() {
    let bridge = AgentBridge::spawn_mock().await.unwrap();
    let resp = bridge.start_session("test_session", ...).await.unwrap();
    assert_eq!(resp.status, "ready");
}
```

### 7.2 Integration Test

Full integration test with real Python agent:
```rust
#[tokio::test]
#[ignore] // Only run with --ignored
async fn test_real_agent() {
    let bridge = AgentBridge::spawn_python().await.unwrap();
    // ... test full workflow
}
```
