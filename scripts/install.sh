#!/usr/bin/env bash
set -e

echo "=== 开始安装 ipfs-tunnels-manager (Linux 用户服务) ==="

# 1. 检查并编译安装二进制
if ! command -v cargo &> /dev/null; then
    echo "错误: 未找到 Rust 环境 (cargo)，请先安装 Rust 工具链。"
    exit 1
fi

echo "正在本地编译并安装二进制文件..."
cargo install --path .

# 2. 创建用户级 systemd 配置目录
SERVICE_DIR="$HOME/.config/systemd/user"
mkdir -p "$SERVICE_DIR"

# 3. 动态写入 systemd 服务文件 (已修正二进制名称)
echo "正在配置 systemd 用户服务..."
cat << EOF > "$SERVICE_DIR/ipfs-tunnels-manager.service"
[Unit]
Description=Declarative IPFS P2P Tunnel Manager (User Service)
After=network.target

[Service]
Type=simple
# 使用 %h 占位符动态指向当前用户的家目录
ExecStart=%h/.cargo/bin/ipfs-tunnels-manager
Restart=on-failure
RestartSec=5s
Environment=RUST_LOG=info

[Install]
WantedBy=default.target
EOF

# 4. 激活服务
echo "正在启动并启用服务..."
systemctl --user daemon-reload
systemctl --user enable ipfs-tunnels-manager
systemctl --user restart ipfs-tunnels-manager

echo "=== 安装完成！ ==="
echo "提示: 你可以使用 'journalctl --user -u ipfs-tunnels-manager -f' 实时查看后台日志。"