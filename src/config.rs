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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::TunnelMode;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    // 辅助函数：利用时间戳在系统临时目录下生成一个唯一的测试路径
    fn get_temp_config_path() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("tunnels_test_{}.conf", nanos))
    }

    #[test]
    fn test_ensure_config_exists_creates_file() {
        let config_path = get_temp_config_path();

        // 首次调用：应该创建文件并写入模板
        assert!(ensure_config_exists(&config_path).is_ok());
        assert!(config_path.exists());

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("mc_client"));
        assert!(content.contains("ssh_server"));

        // 二次调用：不应该覆盖或报错
        assert!(ensure_config_exists(&config_path).is_ok());

        // 清理现场
        let _ = fs::remove_file(config_path);
    }

    #[test]
    fn test_load_desired_state_success() {
        let config_path = get_temp_config_path();

        let mock_config = "\
# 正确的客户端配置
test_cli | client | 192.168.1.5 | 8080 | QmPeerId123 | /x/http | true
# 正确的服务端配置
test_srv | server | 127.0.0.1   | 9090 | -           | /x/grpc | false
";
        fs::write(&config_path, mock_config).unwrap();

        let state = load_desired_state(&config_path).unwrap();

        assert_eq!(state.len(), 2);

        let cli = state.get("/x/http").unwrap();
        assert_eq!(cli.name, "test_cli");
        assert_eq!(cli.mode, TunnelMode::Client);
        assert_eq!(cli.port, 8080);
        assert_eq!(cli.peer_id, "QmPeerId123");
        assert!(cli.enabled);

        let srv = state.get("/x/grpc").unwrap();
        assert_eq!(srv.mode, TunnelMode::Server);
        assert_eq!(srv.port, 9090);
        assert_eq!(srv.peer_id, "-");
        assert!(!srv.enabled);

        let _ = fs::remove_file(config_path);
    }

    #[test]
    fn test_load_desired_state_malformed_skips() {
        let config_path = get_temp_config_path();

        let mock_config = "\
# 错误1：只有 6 列（旧版格式）
old_tunnel | 127.0.0.1 | 22 | - | /x/ssh | true
# 错误2：Mode 填错了
bad_mode   | unknown | 127.0.0.1 | 80 | - | /x/http | true
# 错误3：IP 无法解析
bad_ip     | client  | 999.9.9.9 | 80 | Qm123 | /x/web | true
# 正确的夹杂在中间
good_one   | client  | 10.0.0.1  | 443 | Qm443 | /x/https | true
";
        fs::write(&config_path, mock_config).unwrap();

        let state = load_desired_state(&config_path).unwrap();

        assert_eq!(state.len(), 1);
        assert!(state.contains_key("/x/https"));

        let _ = fs::remove_file(config_path);
    }
}
