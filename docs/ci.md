# GitHub Actions 自动构建配置

本项目使用 GitHub Actions 自动为多个平台构建二进制文件。

## 支持的平台

- **Linux x86_64**: 通用 Linux 发行版
- **Linux aarch64**: ARM64 设备（Armbian、树莓派等）⭐
- **macOS x86_64**: Intel Mac
- **macOS aarch64**: Apple Silicon Mac

## 触发方式

### 1. 推送 tag 自动构建

```bash
git tag v0.1.1-alpha
git push origin v0.1.1-alpha
```

### 2. 手动触发

在 GitHub 仓库页面:
1. 进入 **Actions** 标签
2. 选择 **Release Build** workflow
3. 点击 **Run workflow**
4. 选择分支（通常是 main）
5. 点击绿色的 **Run workflow** 按钮

## 构建产物

所有构建完成后，二进制文件会自动上传到对应的 GitHub Release：

- `hermes-gateway-linux-x86_64.tar.gz`
- `hermes-gateway-linux-aarch64.tar.gz` ⭐ (用于 Armbian)
- `hermes-gateway-macos-x86_64.tar.gz`
- `hermes-gateway-macos-aarch64.tar.gz`

## 在 Armbian 上使用

```bash
# 下载 ARM64 版本
wget https://github.com/YsLtr/hermes-hybrid/releases/download/v0.1.0-alpha/hermes-gateway-linux-aarch64.tar.gz

# 解压
tar -xzf hermes-gateway-linux-aarch64.tar.gz

# 赋予执行权限
chmod +x hermes-gateway

# 运行
./hermes-gateway
```

## 构建优化

- **缓存**: Cargo registry、git index 和 target 目录会被缓存，加速后续构建
- **Strip**: 自动去除调试符号，减小二进制文件大小
- **压缩**: 使用 tar.gz 压缩，便于下载和传输

## 本地测试 workflow

```bash
# 安装 act (GitHub Actions 本地运行工具)
sudo pacman -S act

# 本地运行 workflow
act -j build
```

## 手动交叉编译（备选方案）

如果需要本地构建 ARM64 版本：

```bash
# 安装工具链
rustup target add aarch64-unknown-linux-gnu
sudo pacman -S aarch64-linux-gnu-gcc

# 构建
cd gateway
cargo build --release --target aarch64-unknown-linux-gnu

# 产物位于
ls -lh target/aarch64-unknown-linux-gnu/release/hermes-gateway
```
