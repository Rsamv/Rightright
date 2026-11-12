use serde::{Deserialize, Serialize};
use std::path::PathBuf;

fn default_hotkey() -> String {
    "ctrl+shift+t".to_string()
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    /// Bing 语言代码，例如 zh-Hans / en / ja
    pub target_lang: String,
    /// 全局翻译快捷键，例如 ctrl+shift+t
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            target_lang: "zh-Hans".to_string(),
            hotkey: default_hotkey(),
        }
    }
}

fn config_path() -> PathBuf {
    let base = std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    base.join("Rightright").join("config.json")
}

pub fn load() -> Config {
    let path = config_path();
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(cfg: &Config) {
    let path = config_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(s) = serde_json::to_string_pretty(cfg) {
        let _ = std::fs::write(&path, s);
    }
}

/// (显示名, Bing 代码) —— 顺序即下拉框顺序
pub const LANGS: &[(&str, &str)] = &[
    ("简体中文", "zh-Hans"),
    ("繁體中文", "zh-Hant"),
    ("English", "en"),
    ("日本語", "ja"),
    ("한국어", "ko"),
    ("Français", "fr"),
    ("Deutsch", "de"),
    ("Español", "es"),
    ("Русский", "ru"),
    ("Português", "pt"),
    ("Italiano", "it"),
    ("ไทย", "th"),
    ("Tiếng Việt", "vi"),
    ("العربية", "ar"),
];

/// Bing 代码 -> Google 网页端点代码
pub fn to_google_code(bing: &str) -> &str {
    match bing {
        "zh-Hans" => "zh-CN",
        "zh-Hant" => "zh-TW",
        other => other,
    }
}

/// Bing 代码 -> 有道 demo 端点代码
pub fn to_youdao_code(bing: &str) -> &str {
    match bing {
        "zh-Hans" => "zh-CHS",
        "zh-Hant" => "zh-CHT",
        other => other,
    }
}

/// Bing 代码 -> MyMemory 目标代码
pub fn to_mymemory_code(bing: &str) -> &str {
    match bing {
        "zh-Hans" => "zh-CN",
        "zh-Hant" => "zh-TW",
        other => other,
    }
}