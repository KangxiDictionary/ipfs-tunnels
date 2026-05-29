use crate::models::{DesiredTunnel, TunnelMode};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::net::IpAddr;
use std::path::PathBuf;

pub fn ensure_config_exists(config_file: &PathBuf) -> anyhow::Result<()> {
    if config_file.exists() { return Ok(()); }
    if let Some(parent) = config_file.parent() { fs::create_dir_all(parent)?; }
    let mut file = File::create(config_file)?;

    // 👈 模板扩充至 7 列，增加 mode(client/server)
    let template = "\
# name | mode | local_ip | port | peer_id | protocol | enabled
mc_client  | client | 127.0.0.1 | 25565 | 12D3Koo... | /x/minecraft | true
ssh_server | server | 127.0.0.1 | 22    | -          | /x/ssh       | true
";
    file.write_all(template.as_bytes())?;
    Ok(())
}

pub fn load_desired_state(config_file: &PathBuf) -> anyhow::Result<HashMap<String, DesiredTunnel>> {
    let file = File::open(config_file)?;
    let reader = BufReader::new(file);
    let mut map = HashMap::new();

    for line in reader.lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }

        let parts: Vec<&str> = line.split('|').map(|s| s.trim()).collect();

        // 👈 要求必须是 7 列规范格式 (如果是之前旧版6列，你可以手动加点向后兼容代码，这里以最严谨的7列为例)
        if parts.len() != 7 {
            tracing::error!("配置格式错误: 期望 7 列，但检测到 {} 列 -> {}", parts.len(), line);
            continue;
        }

        let mode = match parts[1].to_lowercase().as_str() {
            "client" => TunnelMode::Client,
            "server" => TunnelMode::Server,
            _ => continue,
        };

        let local_ip: IpAddr = match parts[2].parse() { Ok(v) => v, Err(_) => continue };
        let port: u16 = match parts[3].parse() { Ok(v) => v, Err(_) => continue };
        let enabled: bool = parts[6].parse().unwrap_or(false);

        let tunnel = DesiredTunnel {
            name: parts[0].to_string(),
            mode,
            local_ip,
            port,
            peer_id: parts[4].to_string(), // Server 模式下通常填 "-"
            protocol: parts[5].to_string(),
            enabled,
        };

        map.insert(tunnel.protocol.clone(), tunnel);
    }
    Ok(map)
}
