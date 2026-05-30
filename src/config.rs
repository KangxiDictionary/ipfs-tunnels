use crate::models::{DesiredTunnel, TunnelMode};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::net::IpAddr;
use std::path::PathBuf;

pub fn ensure_config_exists(config_file: &PathBuf) -> anyhow::Result<()> {
    if config_file.exists() {
        return Ok(());
    }
    if let Some(parent) = config_file.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = File::create(config_file)?;

    let template = "\
# name | mode | local_ip | port | peer_id | protocol | enabled
mc_client  | client | 127.0.0.1 | 25565 | 12D3Koo... | /x/minecraft | true
ssh_server | server | 127.0.0.1 | 22    | -          | /x/ssh       | true
";
    file.write_all(template.as_bytes())?;
    Ok(())
}

/// 期望状态现改用 name 作为主键 HashMap，防止配置项因相同协议而被默默吞掉
pub fn load_desired_state(config_file: &PathBuf) -> anyhow::Result<HashMap<String, DesiredTunnel>> {
    let file = File::open(config_file)?;
    let reader = BufReader::new(file);
    let mut map = HashMap::new();
    let mut warning_count = 0;

    for (index, line) in reader.lines().enumerate() {
        let line_num = index + 1;
        let line = line?;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let parts: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
        if parts.len() < 7 {
            tracing::error!("第 {} 行配置格式错误: 列数不足（预期 7 列，实际 {} 列）", line_num, parts.len());
            warning_count += 1;
            continue;
        }

        let name = parts[0].to_string();
        let mode = match parts[1] {
            "client" => TunnelMode::Client,
            "server" => TunnelMode::Server,
            other => {
                tracing::error!("第 {} 行配置解析失败: 未知的模式类型 [{}]", line_num, other);
                warning_count += 1;
                continue;
            }
        };

        let local_ip: IpAddr = match parts[2].parse() {
            Ok(ip) => ip,
            Err(e) => {
                tracing::error!("第 {} 行配置解析失败: IP 地址 [{}] 格式非法 ({})", line_num, parts[2], e);
                warning_count += 1;
                continue;
            }
        };

        let port: u16 = match parts[3].parse() {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("第 {} 行配置解析失败: 端口 [{}] 无法解析 ({})", line_num, parts[3], e);
                warning_count += 1;
                continue;
            }
        };

        let peer_id = parts[4].to_string();
        let protocol = parts[5].to_string();

        let enabled = match parts[6].parse() {
            Ok(b) => b,
            Err(e) => {
                tracing::error!("第 {} 行配置解析失败: enabled 开关 [{}] 必须为 true/false ({})", line_num, parts[6], e);
                warning_count += 1;
                continue;
            }
        };

        let tunnel = DesiredTunnel {
            name: name.clone(),
            mode,
            local_ip,
            port,
            peer_id,
            protocol,
            enabled,
        };

        // ✅ 修复：只在重复时计数
        if map.insert(name.clone(), tunnel).is_some() {
            tracing::warn!("第 {} 行发现重复的隧道名称 [{}]，旧的配置项已被覆盖！", line_num, name);
            warning_count += 1;  // 现在只在重复时递增
        }
    }

    if warning_count > 0 {
        tracing::warn!("配置文件解析完成，期间共检测到 {} 处语法警告/错误，请检查核实。", warning_count);
    }

    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_load_desired_state_valid_config() {
        let temp_dir = TempDir::new().unwrap();
        let config_file = temp_dir.path().join("tunnels.conf");

        let mut file = File::create(&config_file).unwrap();
        file.write_all(b"# name | mode | local_ip | port | peer_id | protocol | enabled\n").unwrap();
        file.write_all(b"mc_client | client | 127.0.0.1 | 25565 | Qm123... | /x/minecraft | true\n").unwrap();

        let result = load_desired_state(&config_file).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result.contains_key("mc_client"));
    }

    #[test]
    fn test_load_desired_state_duplicate_names() {
        let temp_dir = TempDir::new().unwrap();
        let config_file = temp_dir.path().join("tunnels.conf");

        let mut file = File::create(&config_file).unwrap();
        file.write_all(b"# name | mode | local_ip | port | peer_id | protocol | enabled\n").unwrap();
        file.write_all(b"ssh | server | 127.0.0.1 | 22 | - | /x/ssh | true\n").unwrap();
        file.write_all(b"ssh | server | 127.0.0.1 | 23 | - | /x/ssh2 | true\n").unwrap();

        let result = load_desired_state(&config_file).unwrap();
        // 重复的名字会被覆盖，只有最后一个保留
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_load_desired_state_invalid_port() {
        let temp_dir = TempDir::new().unwrap();
        let config_file = temp_dir.path().join("tunnels.conf");

        let mut file = File::create(&config_file).unwrap();
        file.write_all(b"# name | mode | local_ip | port | peer_id | protocol | enabled\n").unwrap();
        file.write_all(b"test | client | 127.0.0.1 | invalid | Qm123 | /x/test | true\n").unwrap();

        let result = load_desired_state(&config_file).unwrap();
        // 非法端口行被跳过
        assert_eq!(result.len(), 0);
    }
}