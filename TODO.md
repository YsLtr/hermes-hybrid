# Hermes Hybrid - TODO List

## Phase 3: Python Agent Bridge Adapter ⏳

**优先级**: 🔥 High

- [ ] 创建 `agent/hermes_cli/agent_bridge.py`
  - [ ] 实现 `AgentBridgeServer` 类
  - [ ] stdin/stdout JSON-RPC 服务器
  - [ ] 请求解析和路由
  - [ ] 响应/通知发送
  
- [ ] 修改 `agent/agent/ai_agent.py`
  - [ ] 添加 `run_stream()` 方法支持流式回调
  - [ ] 工具执行时触发回调（`tool_started`, `tool_completed`）
  - [ ] LLM 流式输出时触发回调（`stream_chunk`）
  
- [ ] 实现协议方法
  - [ ] `start_session`: 初始化 agent
  - [ ] `handle_message`: 运行 agent loop
  - [ ] `interrupt`: 中断执行
  - [ ] `end_session`: 清理会话
  - [ ] `ping`: 心跳响应
  
- [ ] 测试
  - [ ] 本地 Python 脚本测试（模拟 stdin/stdout）
  - [ ] 与 Rust gateway 端到端集成测试

**预计时间**: 1-2天

---

## Phase 4: QQBot Adapter 增强 ⏳

**优先级**: 🔥 High

- [ ] C2C 流式协议
  - [ ] 实现 `send_c2c_stream_chunk()`
  - [ ] 维护 per-chat stream state
  - [ ] Markdown fallback 机制
  
- [ ] Progress Card 合并
  - [ ] 实现 `ProgressCardManager`
  - [ ] 缓冲工具进度行
  - [ ] 3s 节流 + 最多 2 条/轮限制
  - [ ] 生成 markdown 进度卡片
  
- [ ] 流式完成通知
  - [ ] 实现 `StreamEndNotifier`
  - [ ] 发送 "✅ 已完成" 消息
  - [ ] 3s 最小间隔 + 最多 3 次/5 轮
  - [ ] 可配置开关
  
- [ ] 元数据脚注
  - [ ] 实现 `format_metadata_footer()`
  - [ ] 追加到最终消息
  - [ ] 包含模型、provider、TTFT、总时间、工具数、tokens

**预计时间**: 2-3天

---

## Phase 5: Gateway 集成 ⏳

**优先级**: 🔥 High

- [ ] 修改 `gateway/src/main.rs`
  - [ ] 集成 `AgentBridge` 初始化
  - [ ] 启动时启动 agent bridge
  
- [ ] 修改 `gateway/src/gateway.rs`
  - [ ] `route_inbound_message()` 订阅 agent notifications
  - [ ] 处理不同类型的 notification
  - [ ] 调用平台 adapter 相应方法
  
- [ ] Notification 处理逻辑
  - [ ] `TypingStart` → `adapter.send_typing()`
  - [ ] `StreamChunk` → QQBot 使用 C2C stream，其他平台 buffer + edit
  - [ ] `ToolStarted` → 添加到 progress buffer
  - [ ] `ToolCompleted` → 更新 progress card
  - [ ] `MessageComplete` → 发送最终消息 + metadata footer
  - [ ] `Error` → 发送错误消息给用户

**预计时间**: 1天

---

## Phase 6: 测试与调试 ⏳

**优先级**: 🔥 High

- [ ] 单元测试
  - [ ] Gateway agent_bridge 测试
  - [ ] Python agent_bridge 测试
  
- [ ] 集成测试
  - [ ] 端到端消息流测试
  - [ ] 流式输出测试
  - [ ] 工具调用进度测试
  - [ ] 错误处理测试
  
- [ ] Armbian 部署测试
  - [ ] 编译 ARM 二进制
  - [ ] 部署到 Armbian 机器
  - [ ] QQ bot 实际测试
  - [ ] 性能和内存监控
  
- [ ] 压力测试
  - [ ] 并发 session 测试
  - [ ] 长时间运行测试
  - [ ] 内存泄漏检测

**预计时间**: 1-2天

---

## 未来增强 💡

### 性能优化

- [ ] 零拷贝消息传递（共享内存）
- [ ] Agent pool（多 worker 进程）
- [ ] LLM 响应缓存
- [ ] 连接池优化

### 功能扩展

- [ ] 更多平台支持
  - [ ] 微信（企业微信）
  - [ ] 飞书
  - [ ] 钉钉
  
- [ ] 管理界面
  - [ ] Web dashboard
  - [ ] 实时监控
  - [ ] 配置管理
  
- [ ] 分布式部署
  - [ ] Gateway 集群
  - [ ] Agent 集群
  - [ ] Redis session store

### 工具和生态

- [ ] CI/CD pipeline
  - [ ] 自动化测试
  - [ ] 自动化构建（多架构）
  - [ ] 自动化发布
  
- [ ] 文档完善
  - [ ] API 文档
  - [ ] 开发指南
  - [ ] 贡献指南
  - [ ] FAQ

---

## 已完成 ✅

### Phase 1-2: 桥接协议设计 + Rust 实现

- [x] 设计 JSON-RPC 2.0 协议（`docs/protocol.md`）
- [x] 实现 `AgentBridge` 主结构体
- [x] 实现 `AgentSubprocess` 子进程管理
- [x] 实现 JSON-RPC 协议类型定义
- [x] 实现 Notification 类型和解析
- [x] 编译验证通过
- [x] 创建独立仓库 `hermes-hybrid`
- [x] 编写架构文档
- [x] 编写部署指南
- [x] 创建启动/停止脚本
- [x] 创建 systemd 服务配置

**完成日期**: 2026-06-29

---

## 优先级说明

- 🔥 High: 核心功能，阻塞发布
- ⭐ Medium: 重要功能，不阻塞发布
- 💡 Low: 增强功能，未来考虑

---

## 时间估算

| Phase | 状态 | 预计时间 | 实际时间 |
|-------|------|---------|---------|
| 1-2: 协议 + Rust | ✅ | 1天 | 1天 |
| 3: Python bridge | ⏳ | 1-2天 | - |
| 4: QQBot 增强 | ⏳ | 2-3天 | - |
| 5: Gateway 集成 | ⏳ | 1天 | - |
| 6: 测试调试 | ⏳ | 1-2天 | - |
| **总计** | | **6-9天** | **1天** |

**当前进度**: 20% (2/10 天完成)

---

## 更新日志

- **2026-06-29**: 创建 hermes-hybrid 仓库，完成 Phase 1-2
