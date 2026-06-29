# Hermes Hybrid

**混合架构的 AI Agent 系统：Rust Gateway + Python Agent**

高性能消息网关（Rust）+ 成熟 Agent 引擎（Python）的完美结合。

---

## 架构概览

```
┌─────────────────────────────────────────────────────┐
│  Rust Gateway (gateway/)                             │
│  - 多平台适配器（QQ、Telegram、Discord 等）          │
│  - 低内存占用（~30MB）                               │
│  - 高性能消息路由                                     │
│  - JSON-RPC 2.0 桥接协议                            │
└────────────────┬────────────────────────────────────┘
                 │ stdin/stdout (JSON-RPC)
                 │
┌────────────────┴────────────────────────────────────┐
│  Python Agent (agent/)                               │
│  - Agent loop, LLM providers                         │
│  - 30+ 工具后端                                       │
│  - Memory & Skills 管理                              │
│  - 上下文管理                                         │
└─────────────────────────────────────────────────────┘
```

## 特性

- 🚀 **高性能**: Rust gateway 处理所有 I/O，Python agent 专注智能
- 🔌 **多平台**: QQ、Telegram、Discord、Slack 等 17+ 平台
- 🧠 **功能完整**: 复用原版 Hermes Agent 的所有功能
- ⚡ **低资源**: 适合 ARM 设备（树莓派、Armbian 等）
- 🔄 **流式输出**: 实时 token streaming，工具执行进度反馈
- 📦 **易部署**: 单一 systemd 服务，自动重启

## 快速开始

### 前置要求

- Rust 1.75+ (用于编译 gateway)
- Python 3.10+ (用于运行 agent)
- Git

### 安装

```bash
# 1. 克隆仓库
git clone https://github.com/yourusername/hermes-hybrid.git
cd hermes-hybrid

# 2. 编译 Rust gateway
cd gateway
cargo build --release

# 3. 安装 Python agent 依赖
cd ../agent
pip install -r requirements.txt

# 4. 配置
cp config.example.yaml config.yaml
vim config.yaml  # 填写 API keys 和平台配置

# 5. 启动
cd ..
./scripts/start.sh
```

### Docker 部署

```bash
docker-compose up -d
```

### systemd 服务

```bash
sudo cp systemd/hermes-hybrid.service /etc/systemd/system/
sudo systemctl enable hermes-hybrid.service
sudo systemctl start hermes-hybrid.service
```

## 配置

### Gateway 配置 (gateway/config.yaml)

```yaml
agent_bridge:
  python_path: /usr/bin/python3
  agent_module: hermes_cli.agent_bridge
  working_dir: /path/to/agent
  heartbeat_interval_secs: 30
  request_timeout_secs: 300

platforms:
  qqbot:
    enabled: true
    extra:
      app_id: "your-app-id"
      client_secret: "your-secret"
      c2c_streaming: true
      progress_coalesce: true
      metadata_footer: true
```

### Agent 配置 (agent/.env)

```bash
ANTHROPIC_API_KEY=sk-ant-...
OPENAI_API_KEY=sk-...
```

## 开发

### 项目结构

```
hermes-hybrid/
├── gateway/          # Rust 消息网关
│   ├── src/
│   │   ├── main.rs
│   │   ├── agent_bridge/   # JSON-RPC 桥接
│   │   └── platforms/      # 平台适配器
│   └── Cargo.toml
├── agent/            # Python Agent 引擎
│   ├── hermes_cli/
│   │   └── agent_bridge.py  # 桥接适配器
│   ├── agent/               # Agent loop
│   └── tools/               # 工具后端
└── docs/
    ├── architecture.md      # 架构文档
    └── protocol.md          # 桥接协议
```

### 桥接协议

Gateway 和 Agent 通过 JSON-RPC 2.0 over stdin/stdout 通信：

**Gateway → Agent (请求)**:
```json
{
  "jsonrpc": "2.0",
  "method": "handle_message",
  "params": {
    "session_id": "qqbot_user123",
    "text": "用户消息",
    "attachments": []
  },
  "id": 1
}
```

**Agent → Gateway (通知)**:
```json
{
  "jsonrpc": "2.0",
  "method": "stream_chunk",
  "params": {
    "session_id": "qqbot_user123",
    "text": "AI 回复片段",
    "is_final": false
  }
}
```

详见 [协议文档](docs/protocol.md)。

### 添加新平台

1. 在 `gateway/src/platforms/` 实现 `PlatformAdapter` trait
2. 注册到 `gateway/src/main.rs`
3. 添加配置到 `config.yaml`

### 添加新工具

在 `agent/tools/` 添加工具实现，Agent 自动加载。

## 测试

### Gateway 测试

```bash
cd gateway
cargo test
```

### Agent 测试

```bash
cd agent
pytest tests/
```

### 端到端测试

```bash
./scripts/test-e2e.sh
```

## 性能

在 Armbian (ARMv8, 910MB RAM) 上的测试结果：

| 指标 | Rust Gateway | Python Agent | 总计 |
|------|--------------|--------------|------|
| 内存占用 | ~30MB | ~100MB | ~130MB |
| 启动时间 | <100ms | ~2s | ~2s |
| 消息延迟 | <5ms | 变化 | 取决于 LLM |

## 故障排除

### Gateway 无法启动

检查 Python agent 路径配置：
```bash
cd gateway
cat config.yaml | grep python_path
```

### Agent 未响应

查看日志：
```bash
journalctl -u hermes-hybrid.service -f
```

### JSON-RPC 通信错误

启用调试日志：
```bash
RUST_LOG=debug ./gateway/target/release/hermes-gateway
```

## 贡献

欢迎贡献！请查看 [CONTRIBUTING.md](CONTRIBUTING.md)。

## 许可证

MIT License

## 相关项目

- [hermes-agent](https://github.com/NousResearch/hermes-agent) - 原版 Python 实现
- [hermes-agent-rs](https://github.com/yourusername/hermes-agent-rs) - 纯 Rust 实现（开发中）

## 致谢

- [Nous Research](https://nousresearch.com) - 原版 Hermes Agent
- [Anthropic](https://anthropic.com) - Claude API
- [OpenAI](https://openai.com) - GPT API

---

**Status**: 🚧 Alpha - 核心功能开发中

**Roadmap**:
- [x] 桥接协议设计
- [x] Rust Gateway 基础实现
- [ ] Python Agent 桥接适配器
- [ ] QQBot 增强功能
- [ ] 更多平台支持
- [ ] 性能优化
