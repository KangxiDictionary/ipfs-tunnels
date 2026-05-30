Write-Host "=== 开始安装/更新 ipfs-tunnels-manager (Windows 用户后台任务) ===" -ForegroundColor Cyan

# 1. 检查 Rust 环境
if ((Get-Command "cargo" -ErrorAction SilentlyContinue) -eq $null) {
    Write-Host "错误: 未找到 Rust 环境 (cargo)，请先安装 Rust 工具链。" -ForegroundColor Red
    exit 1
}

Write-Host "正在编译最新版本..." -ForegroundColor Yellow
cargo install --path . --force   # 添加 --force 确保更新

$ExePath = Join-Path $HOME ".cargo\bin\ipfs-tunnels-manager.exe"
if (-not (Test-Path $ExePath)) {
    Write-Host "错误: 编译失败，请检查 cargo 输出。" -ForegroundColor Red
    exit 1
}

$TaskName = "IpfsTunnelsManager"

# 2. 如果任务已存在，先停止它
if (Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue) {
    Write-Host "检测到旧版本任务，正在停止并更新..." -ForegroundColor Yellow
    Stop-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
    Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false
}

# 3. 注册/更新计划任务
Write-Host "正在注册后台常驻任务..." -ForegroundColor Yellow

$Trigger = New-ScheduledTaskTrigger -AtLogOn
$Action = New-ScheduledTaskAction -Execute $ExePath
$Settings = New-ScheduledTaskSettingsSet `
    -AllowStartIfOnBatteries `
    -DontStopIfGoingOnBatteries `
    -ExecutionTimeLimit (New-TimeSpan -Days 365) `
    -RestartCount 3 `
    -RestartInterval (New-TimeSpan -Minutes 1)

Register-ScheduledTask `
    -TaskName $TaskName `
    -Trigger $Trigger `
    -Action $Action `
    -Settings $Settings `
    -Description "Declarative IPFS P2P Tunnel Manager" `
    -Force

# 4. 启动任务
Write-Host "正在启动服务..." -ForegroundColor Yellow
Start-ScheduledTask -TaskName $TaskName

Write-Host "=== 安装/更新完成！ ===" -ForegroundColor Green
Write-Host "任务名称: $TaskName" -ForegroundColor Gray
Write-Host "可执行文件: $ExePath" -ForegroundColor Gray
Write-Host "管理方式: 打开 Windows '任务计划程序' 搜索 '$TaskName'" -ForegroundColor Gray