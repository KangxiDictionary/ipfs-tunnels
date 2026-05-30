Write-Host "=== 开始安装 ipfs-tunnels-manager (Windows 用户后台任务) ===" -ForegroundColor Cyan

# 1. 检查并编译安装二进制
if ((Get-Command "cargo" -ErrorAction SilentlyContinue) -eq $null) {
    Write-Host "错误: 未找到 Rust 环境 (cargo)，请先安装 Rust 工具链。" -ForegroundColor Red
    exit 1
}

Write-Host "正在本地编译并安装二进制文件..." -ForegroundColor Yellow
cargo install --path .

# 2. 计算可执行文件的绝对路径 (已修正为 ipfs-tunnels-manager.exe)
$ExePath = Join-Path $HOME ".cargo\bin\ipfs-tunnels-manager.exe"
if (-not (Test-Path $ExePath)) {
    Write-Host "错误: 编译产物未找到，请检查 cargo install 是否成功。" -ForegroundColor Red
    exit 1
}

# 3. 使用 Windows 原生命令创建后台计划任务
Write-Host "正在向系统注册后台常驻任务..." -ForegroundColor Yellow

$TaskName = "IpfsTunnelsManager"
# 定义触发器：当前用户登录时触发
$Trigger = New-ScheduledTaskTrigger -AtLogOn
# 定义执行动作：运行编译好的二进制文件 (后台隐蔽运行)
$Action = New-ScheduledTaskAction -Execute $ExePath
# 定义运行策略：允许电池供电运行，断网不停止，允许无限期运行
$Settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit (New-TimeSpan -Days 365)

# 注册任务 (-Principal 确保它以当前登录用户身份静默运行，不会弹窗)
Register-ScheduledTask -TaskName $TaskName -Trigger $Trigger -Action $Action -Settings $Settings -Description "Declarative IPFS P2P Tunnel Manager" -Force

# 4. 立即在后台拉起该任务
Write-Host "正在后台启动服务..." -ForegroundColor Yellow
Start-ScheduledTask -TaskName $TaskName

Write-Host "=== 安装完成！ ===" -ForegroundColor Green
Write-Host "提示: 程序已在后台静默运行。你可以在 Windows '任务计划程序' 中随时管理它。" -ForegroundColor Gray