//! Runtime language selection shared by UI and worker threads.
use std::sync::atomic::{AtomicBool, Ordering};

static ENGLISH: AtomicBool = AtomicBool::new(false);

fn language(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "en" | "en-us" | "en-gb" => Some(true),
        "zh" | "zh-cn" | "zh-tw" | "zh-hk" => Some(false),
        _ => None,
    }
}

pub fn init() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let mut selected = None;
    while let Some(arg) = args.next() {
        let value = if arg == "--lang" {
            Some(args.next().ok_or("--lang requires en or zh-CN")?)
        } else {
            arg.strip_prefix("--lang=").map(str::to_owned)
        };
        if let Some(value) = value {
            selected = Some(
                language(&value)
                    .ok_or_else(|| format!("Unsupported language: {value}. Use en or zh-CN."))?,
            );
        }
    }
    let saved = preference_path()
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| language(s.trim()));
    let english = selected.or(saved).unwrap_or_else(|| {
        // Primary language ID 0x04 covers Chinese display-language variants.
        unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() & 0x03ff != 0x04 }
    });
    ENGLISH.store(english, Ordering::Relaxed);
    Ok(())
}

pub fn english() -> bool {
    // Tests that don't initialize the application retain the original Chinese UI.
    ENGLISH.load(Ordering::Relaxed)
}

fn preference_path() -> Result<std::path::PathBuf, String> {
    Ok(crate::storage::config_path()?.with_file_name("language.txt"))
}

pub fn switch(english: bool, persist: bool) -> Result<(), String> {
    if persist {
        save_preference(&preference_path()?, english)?;
    }
    ENGLISH.store(english, Ordering::Relaxed);
    Ok(())
}

fn save_preference(path: &std::path::Path, english: bool) -> Result<(), String> {
    use windows::{Win32::Storage::FileSystem::*, core::PCWSTR};
    std::fs::create_dir_all(path.parent().ok_or("Invalid language preference path")?)
        .map_err(|e| e.to_string())?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, if english { "en" } else { "zh-CN" }).map_err(|e| e.to_string())?;
    let from = crate::storage::wide(&tmp.to_string_lossy());
    let to = crate::storage::wide(&path.to_string_lossy());
    unsafe {
        MoveFileExW(
            PCWSTR(from.as_ptr()),
            PCWSTR(to.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    }
    .map_err(|e| e.to_string())
}

// Notices can outlive the language in which the worker produced them.
pub fn notice(value: &str) -> &str {
    let (zh, en) = match value {
        "配置已保存" | "Profile saved" => ("配置已保存", "Profile saved"),
        "预览：配置已保存（仅内存）" | "Preview: profile saved in memory" => (
            "预览：配置已保存（仅内存）",
            "Preview: profile saved in memory",
        ),
        "预览：已应用保存值（未修改系统）" | "Preview: saved values applied (system unchanged)" => {
            (
                "预览：已应用保存值（未修改系统）",
                "Preview: saved values applied (system unchanged)",
            )
        }
        "预览：关联成功（仅内存）" | "Preview: device linked in memory" => (
            "预览：关联成功（仅内存）",
            "Preview: device linked in memory",
        ),
        "预览模式：不监听输入、不保存配置、不更改系统设置"
        | "Preview mode: no input monitoring, disk writes or system changes" => (
            "预览模式：不监听输入、不保存配置、不更改系统设置",
            "Preview mode: no input monitoring, disk writes or system changes",
        ),
        _ => return value,
    };
    if english() { en } else { zh }
}

macro_rules! text {
    ($zh:literal, $en:literal) => {
        if $crate::i18n::english() { $en } else { $zh }
    };
}
pub(crate) use text;

macro_rules! localized_format {
    ($zh:literal, $en:literal $(, $arg:expr)* $(,)?) => {
        if $crate::i18n::english() {
            format!($en $(, $arg)*)
        } else {
            format!($zh $(, $arg)*)
        }
    };
}
pub(crate) use localized_format;

#[cfg(test)]
pub fn test_english() {
    ENGLISH.store(true, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preference_survives_replacement() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings/language.txt");
        save_preference(&path, true).unwrap();
        assert_eq!(
            language(std::fs::read_to_string(&path).unwrap().trim()),
            Some(true)
        );
        save_preference(&path, false).unwrap();
        assert_eq!(
            language(std::fs::read_to_string(&path).unwrap().trim()),
            Some(false)
        );
        assert!(!path.with_extension("tmp").exists());
    }

    #[test]
    fn parses_supported_languages() {
        assert_eq!(language("EN"), Some(true));
        assert_eq!(language("en-US"), Some(true));
        assert_eq!(language("zh-CN"), Some(false));
        assert_eq!(language("zh-TW"), Some(false));
        assert_eq!(language("invalid"), None);
    }
}
