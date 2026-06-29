# Agent 目录占位符

此目录应包含 Python Hermes Agent 代码。

## 快速开始

### 选项 1: 从 Armbian 复制（推荐用于你的场景）

```bash
# 从 Armbian 机器复制完整 agent 代码
scp -r root@192.168.11.11:/root/.hermes/hermes-agent/* agent/

# 添加 agent_bridge.py
cp scripts/agent_bridge_template.py agent/hermes_cli/agent_bridge.py

# 创建 .env
cd agent
cp .env.example .env
vim .env  # 填写 API keys

# 安装依赖
pip3 install -r requirements.txt
```

### 选项 2: 使用 Git 克隆

```bash
# 克隆原版仓库到临时目录
git clone https://github.com/NousResearch/hermes-agent.git /tmp/hermes-agent

# 复制到 agent 目录
cp -r /tmp/hermes-agent/* agent/

# 添加 agent_bridge.py
cp scripts/agent_bridge_template.py agent/hermes_cli/agent_bridge.py
```

### 选项 3: 使用准备脚本

```bash
export HERMES_AGENT_PATH=/path/to/your/hermes-agent
./scripts/prepare-agent.sh
```

## 必需文件结构

准备完成后，agent 目录应该包含：

```
agent/
├── agent/                  # Agent loop 核心
├── tools/                  # 工具后端
├── hermes_cli/            # CLI 模块
│   └── agent_bridge.py    # 🆕 需要添加
├── gateway/               # 原版 gateway（参考用）
├── requirements.txt       # Python 依赖
├── .env                   # API keys（不提交）
└── .env.example          # 环境变量模板
```

## 下一步

1. 使用上述任一方式准备 agent 目录
2. 实现 `agent/hermes_cli/agent_bridge.py`（参考 `scripts/agent_bridge_template.py`）
3. 配置 `.env` 文件
4. 安装依赖：`pip3 install -r requirements.txt`
5. 测试：`python3 -m hermes_cli.agent_bridge`

---

**提示**: 由于 agent 目录内容较大且包含敏感配置，建议在 `.gitignore` 中保持对 `agent/.env` 的忽略。
