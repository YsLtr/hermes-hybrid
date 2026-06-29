# Hermes Hybrid 部署指南

本文档介绍如何在不同环境下部署 Hermes Hybrid。

---

## 快速开始（本地开发）

### 1. 克隆仓库

```bash
git clone https://github.com/yourusername/hermes-hybrid.git
cd hermes-hybrid
```

### 2. 准备 Gateway (Rust)

```bash
# 从 hermes-agent-rs 复制 agent_bridge 模块
cp -r ../hermes-agent-rs/crates/hermes-gateway/src/agent_bridge gateway/src/

# 创建 gateway/Cargo.toml（参考 hermes-agent-rs）
# 或者直接使用 git submodule

# 编译
cd gateway
cargo build --release
```

### 3. 准备 Agent (Python)

```bash
# 从原版 hermes-agent 复制或使用 git submodule
cp -r /path/to/hermes-agent/* agent/

# 创建 agent_bridge.py
vim agent/hermes_cli/agent_bridge.py
# 参考 docs/progress.md 中的实现框架

# 安装依赖
cd agent
pip install -r requirements.txt
```

### 4. 配置

```bash
# Gateway 配置
cd gateway
cp config.example.yaml config.yaml
vim config.yaml  # 填写 Python agent 路径

# Agent 配置
cd ../agent
cp .env.example .env
vim .env  # 填写 API keys
```

### 5. 启动

```bash
cd ..
./scripts/start.sh
```

---

## Armbian/树莓派部署

适用于低功耗 ARM 设备。

### 系统要求

- **OS**: Armbian / Raspberry Pi OS
- **CPU**: ARMv7+ (推荐 ARMv8)
- **内存**: 最低 512MB，推荐 1GB+
- **存储**: 8GB+
- **网络**: 稳定互联网连接

### 安装步骤

#### 1. 安装依赖

```bash
# 更新系统
sudo apt update && sudo apt upgrade -y

# 安装 Rust (如果要本地编译)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# 安装 Python 3.10+
sudo apt install python3 python3-pip python3-venv -y

# 安装其他依赖
sudo apt install git build-essential pkg-config libssl-dev -y
```

#### 2. 克隆仓库

```bash
cd /root
git clone https://github.com/yourusername/hermes-hybrid.git
cd hermes-hybrid
```

#### 3. 编译 Gateway

**选项 A: 本地编译**（慢，但适配性好）
```bash
cd gateway
cargo build --release
```

**选项 B: 交叉编译**（快，但需要在 x86 机器上）
```bash
# 在 x86 机器上
rustup target add aarch64-unknown-linux-gnu
cargo build --release --target aarch64-unknown-linux-gnu

# 传输到 ARM 设备
scp target/aarch64-unknown-linux-gnu/release/hermes-gateway root@192.168.11.11:/root/hermes-hybrid/gateway/target/release/
```

**选项 C: 使用预编译二进制**（最快）
```bash
wget https://github.com/yourusername/hermes-hybrid/releases/download/v0.1.0/hermes-gateway-aarch64
chmod +x hermes-gateway-aarch64
mv hermes-gateway-aarch64 gateway/target/release/hermes-gateway
```

#### 4. 安装 Python Agent

```bash
cd agent
pip3 install -r requirements.txt
```

#### 5. 配置

```bash
# Gateway 配置
cd ../gateway
cp config.example.yaml config.yaml
vim config.yaml

# 修改以下配置
# agent_bridge:
#   python_path: /usr/bin/python3
#   agent_module: hermes_cli.agent_bridge
#   working_dir: /root/hermes-hybrid/agent

# Agent 配置
cd ../agent
vim .env

# ANTHROPIC_API_KEY=sk-ant-...
# OPENAI_API_KEY=sk-...
```

#### 6. 测试运行

```bash
cd /root/hermes-hybrid
./scripts/start.sh
```

观察日志，确认无错误。按 `Ctrl+C` 停止。

#### 7. 安装 systemd 服务

```bash
# 修改 systemd 服务文件中的路径
sudo vim systemd/hermes-hybrid.service

# 复制到 systemd
sudo cp systemd/hermes-hybrid.service /etc/systemd/system/

# 重载配置
sudo systemctl daemon-reload

# 启用自动启动
sudo systemctl enable hermes-hybrid.service

# 启动服务
sudo systemctl start hermes-hybrid.service

# 查看状态
sudo systemctl status hermes-hybrid.service

# 查看日志
sudo journalctl -u hermes-hybrid.service -f
```

#### 8. 性能优化（可选）

**减少内存占用**:
```yaml
# gateway/config.yaml
agent_bridge:
  max_concurrent_sessions: 3  # 限制并发 session 数
```

**启用 swap**（如果内存不足）:
```bash
sudo fallocate -l 1G /swapfile
sudo chmod 600 /swapfile
sudo mkswap /swapfile
sudo swapon /swapfile
echo '/swapfile none swap sw 0 0' | sudo tee -a /etc/fstab
```

---

## Docker 部署

适用于容器化环境。

### Dockerfile (Gateway)

```dockerfile
# gateway/Dockerfile
FROM rust:1.75 as builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    python3 \
    python3-pip \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/hermes-gateway /usr/local/bin/
COPY config.yaml /etc/hermes/config.yaml

WORKDIR /app
CMD ["hermes-gateway"]
```

### docker-compose.yml

```yaml
version: '3.8'

services:
  hermes-hybrid:
    build:
      context: ./gateway
      dockerfile: Dockerfile
    volumes:
      - ./gateway/config.yaml:/etc/hermes/config.yaml:ro
      - ./agent:/app/agent:ro
    environment:
      - RUST_LOG=info
      - PYTHONPATH=/app/agent
    restart: unless-stopped
    networks:
      - hermes-net

networks:
  hermes-net:
    driver: bridge
```

### 启动

```bash
docker-compose up -d
docker-compose logs -f
```

---

## 云服务器部署

适用于 VPS、云虚拟机等。

### 推荐配置

- **CPU**: 1 核心+
- **内存**: 1GB+
- **存储**: 10GB+
- **OS**: Ubuntu 22.04 LTS / Debian 12

### 安装脚本

创建 `deploy.sh`:

```bash
#!/bin/bash
set -e

echo "🚀 Deploying Hermes Hybrid..."

# 安装依赖
apt update && apt install -y git python3 python3-pip curl build-essential

# 安装 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source $HOME/.cargo/env

# 克隆仓库
cd /opt
git clone https://github.com/yourusername/hermes-hybrid.git
cd hermes-hybrid

# 编译 gateway
cd gateway
cargo build --release

# 安装 agent
cd ../agent
pip3 install -r requirements.txt

# 配置
cd ..
cp gateway/config.example.yaml gateway/config.yaml
echo "Please edit gateway/config.yaml and agent/.env"

# 安装 systemd 服务
cp systemd/hermes-hybrid.service /etc/systemd/system/
systemctl daemon-reload
systemctl enable hermes-hybrid.service

echo "✅ Deployment complete. Edit config files and run:"
echo "   systemctl start hermes-hybrid.service"
```

运行：
```bash
bash deploy.sh
```

---

## 生产环境最佳实践

### 1. 安全

**防火墙**:
```bash
# 只开放必要端口（如果有 API server）
ufw allow 8080/tcp
ufw enable
```

**文件权限**:
```bash
chmod 600 gateway/config.yaml agent/.env
chown root:root systemd/hermes-hybrid.service
```

**日志脱敏**: 确保配置中启用了 `log_redact: true`

### 2. 监控

**Prometheus 指标**:
```yaml
# gateway/config.yaml
telemetry:
  metrics:
    enabled: true
    port: 9090
```

**日志采集**:
```bash
# 使用 journald 或 rsyslog
journalctl -u hermes-hybrid.service -o json | your-log-collector
```

**健康检查**:
```bash
# 添加到监控系统
curl http://localhost:8080/health
```

### 3. 备份

**配置备份**:
```bash
# 定期备份配置
tar -czf hermes-config-$(date +%Y%m%d).tar.gz \
    gateway/config.yaml \
    agent/.env \
    agent/.hermes/memories/
```

**数据库备份**（如果使用）:
```bash
sqlite3 agent/.hermes/sessions.db .dump > sessions-backup.sql
```

### 4. 更新

**滚动更新**:
```bash
# 拉取最新代码
cd /opt/hermes-hybrid
git pull

# 重新编译 gateway
cd gateway
cargo build --release

# 更新 agent
cd ../agent
pip install -U -r requirements.txt

# 重启服务
systemctl restart hermes-hybrid.service

# 验证
systemctl status hermes-hybrid.service
```

**版本管理**:
```bash
# 使用 git tags
git tag -a v0.2.0 -m "Release v0.2.0"
git push origin v0.2.0
```

---

## 故障排除

### Gateway 无法启动

**检查日志**:
```bash
journalctl -u hermes-hybrid.service -n 50
```

**常见问题**:
- Python 路径错误：检查 `config.yaml` 中的 `python_path`
- 端口占用：检查其他进程是否占用端口
- 权限问题：确保二进制文件有执行权限

### Agent 无响应

**检查 Python 进程**:
```bash
ps aux | grep agent_bridge
```

**手动测试 Agent**:
```bash
cd agent
python3 -m hermes_cli.agent_bridge
# 输入 JSON-RPC 请求测试
```

### 内存不足

**检查内存使用**:
```bash
free -h
ps aux --sort=-%mem | head -10
```

**优化措施**:
- 减少并发 session 数
- 启用 swap
- 升级内存

### 高延迟

**检查网络延迟**:
```bash
ping api.anthropic.com
traceroute api.anthropic.com
```

**优化措施**:
- 使用地理位置更近的服务器
- 启用 LLM 响应缓存
- 调整超时配置

---

## 多实例部署

如果需要高可用或负载均衡：

### 架构

```
         Load Balancer (Nginx)
                 |
    +------------+------------+
    |            |            |
Gateway 1    Gateway 2    Gateway 3
    |            |            |
    +------------+------------+
                 |
          Agent Pool (共享)
```

### Nginx 配置

```nginx
upstream hermes_gateway {
    server 192.168.1.10:8080;
    server 192.168.1.11:8080;
    server 192.168.1.12:8080;
}

server {
    listen 80;
    location / {
        proxy_pass http://hermes_gateway;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
    }
}
```

### Session 亲和性

当前版本需要 session 亲和性（同一用户请求路由到同一 gateway）：

```nginx
upstream hermes_gateway {
    ip_hash;  # 基于 IP 的亲和性
    server 192.168.1.10:8080;
    server 192.168.1.11:8080;
}
```

---

## 下一步

部署完成后：

1. 测试基本消息收发
2. 测试工具调用
3. 测试流式输出
4. 配置监控和告警
5. 定期备份

有问题？查看 [FAQ](FAQ.md) 或提交 [Issue](https://github.com/yourusername/hermes-hybrid/issues)。
