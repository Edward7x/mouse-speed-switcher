use crate::model::Config;
use std::{
    fs,
    path::{Path, PathBuf},
};
use windows::{
    Win32::Storage::FileSystem::{MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW},
    core::PCWSTR,
};

pub fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
pub fn config_path() -> Result<PathBuf, String> {
    std::env::var_os("LOCALAPPDATA")
        .map(|p| {
            PathBuf::from(p)
                .join("MouseSpeedSwitcher")
                .join("config.json")
        })
        .ok_or_else(|| crate::i18n::text!("找不到 LOCALAPPDATA", "LOCALAPPDATA is not set").into())
}
fn decode(path: &Path) -> Result<Config, String> {
    let c: Config = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    c.validate()?;
    Ok(c)
}
pub fn load(path: &Path) -> Result<(Config, Option<String>), String> {
    if !path.exists() {
        return Ok((Config::default(), None));
    }
    match decode(path) {
        Ok(c) => Ok((c, None)),
        Err(error) => {
            // Preserve the damaged original before any subsequent save.
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let damaged = path.with_extension(format!("corrupt-{stamp}.json"));
            fs::copy(path, &damaged).map_err(|e| {
                crate::i18n::localized_format!(
                    "无法保留损坏的配置：{e}",
                    "Cannot preserve damaged configuration: {e}"
                )
            })?;
            let recovered = decode(&path.with_extension("json.bak"));
            let (config, action) = match recovered {
                Ok(c) => (c, crate::i18n::text!("已载入备份", "Backup loaded")),
                Err(_) => (
                    Config::default(),
                    crate::i18n::text!("已使用空配置", "Started with an empty configuration"),
                ),
            };
            Ok((
                config,
                Some(crate::i18n::localized_format!(
                    "配置读取失败：{error}；{action}。原文件已保留至 {}",
                    "Cannot read configuration: {error}; {action}. Original preserved at {}",
                    damaged.display()
                )),
            ))
        }
    }
}
pub fn save(path: &Path, config: &Config) -> Result<(), String> {
    config.validate()?;
    fs::create_dir_all(path.parent().ok_or(crate::i18n::text!(
        "配置路径无效",
        "Invalid configuration path"
    ))?)
    .map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    let data = serde_json::to_vec_pretty(config).map_err(|e| e.to_string())?;
    {
        use std::io::Write;
        let mut f = fs::File::create(&tmp).map_err(|e| e.to_string())?;
        f.write_all(&data)
            .and_then(|_| f.sync_all())
            .map_err(|e| e.to_string())?;
    }
    if path.exists() && decode(path).is_ok() {
        fs::copy(path, path.with_extension("json.bak")).map_err(|e| e.to_string())?;
    }
    let from = wide(&tmp.to_string_lossy());
    let to = wide(&path.to_string_lossy());
    unsafe {
        MoveFileExW(
            PCWSTR(from.as_ptr()),
            PCWSTR(to.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    }
    .map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn atomic_round_trip_and_corrupt_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("config.json");
        let c = Config::default();
        save(&p, &c).unwrap();
        save(&p, &c).unwrap();
        assert!(load(&p).unwrap().1.is_none());
        fs::write(&p, "broken").unwrap();
        let (_, warning) = load(&p).unwrap();
        assert!(warning.is_some());
        assert_eq!(fs::read_to_string(&p).unwrap(), "broken");
        assert!(fs::read_dir(dir.path()).unwrap().any(|e| {
            e.unwrap()
                .file_name()
                .to_string_lossy()
                .contains("corrupt-")
        }));
    }
    #[test]
    fn invalid_values_and_future_versions_rejected() {
        let mut c = Config {
            version: 99,
            ..Config::default()
        };
        assert!(c.validate().is_err());
        c.version = 1;
        c.bindings.insert("a".into(), "missing".into());
        assert!(c.validate().is_err());
    }
}
