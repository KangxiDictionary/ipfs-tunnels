#!/usr/bin/env bash
set -e

echo "=== 开始安装/更新 ipfs-tunnels-manager (Linux 用户服务) ==="

if ! command -v cargo &> /dev/null; then
    echo "错误: 未找到 Rust 环境 (cargo)..."
    exit 1
fi

echo "正在编译最新版本..."
cargo install --path . --force   # 添加 --force 更明确

SERVICE_DIR="$HOME/.config/systemd/user"
mkdir -p "$SERVICE_DIR"

echo "正在更新 systemd 服务文件..."
cat << EOF > "$SERVICE_DIR/ipfs-tunnels-manager.service"
[Unit]
Description=Declarative IPFS P2P Tunnel Manager (User Service)
After=network.target

[Service]
Type=simple
ExecStart=%h/.cargo/bin/ipfs-tunnels-manager
Restart=on-failure
RestartSec=5s
Environment=RUST_LOG=info

[Install]
WantedBy=default.target
EOF

echo "正在重新加载并启动服务..."
systemctl --user daemon-reload
systemctl --user enable --now ipfs-tunnels-manager
systemctl --user restart ipfs-tunnels-manager

echo "=== 更新/安装完成！ ==="
echo "当前运行版本：$(~/.cargo/bin/ipfs-tunnels-manager --version 2>/dev/null || echo '未知')"
echo "查看日志: journalctl --user -u ipfs-tunnels-manager -f"