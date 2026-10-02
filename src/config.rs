//! 配置层：目录扫描 + manifest 结构 + 校验 + 端口自动分配。
//!
//! 目录约定（`lyco/v1`，见 SPEC.md）：
//! ```text
//! <root>/
//!   services/<name>/service.toml
//!   workflows/<name>/workflow.toml
//!   templates/<name>.toml
//!   models/catalog.toml
//! ```
//! **文件夹名 = name = 全局唯一 ID。加文件即新增，删文件即移除。**

use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub const API_VERSION: &str = "lyco/v1";
/// 前端端口池（顾客永不用手填）
pub const LISTEN_POOL: (u16, u16) = (7300, 7399);
/// 后端端口池
pub const BACKEND_POOL: (u16, u16) = (7400, 7499);
/// 能力词表（SPEC §4）
pub const KINDS: &[&str] = &[
    "asr", "tts", "ocr", "rembg", "llm", "download", "media", "custom",
];

fn api_ver() -> String {
    API_VERSION.to_string()
}
fn d_mode() -> String {
    "local".into()
}
fn d_idle() -> u64 {
    300
}
fn d_ready() -> u64 {
    30
}

// ────────────────────────── service ──────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct ServiceManifest {
    #[serde(default = "api_ver")]
    pub api_version: String,
    pub service: ServiceMeta,
    #[serde(default)]
    pub runtime: Option<Runtime>,
    #[serde(default)]
    pub remote: Option<Remote>,
    #[serde(default)]
    pub params: Option<toml::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServiceMeta {
    pub name: String,
    pub kind: String,
    #[serde(default = "d_mode")]
    pub mode: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub about: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Runtime {
    pub listen: String,
    pub backend: String,
    #[serde(default)]
    pub up: String,
    #[serde(default)]
    pub down: String,
    #[serde(default = "d_idle")]
    pub idle_timeout_secs: u64,
    #[serde(default = "d_ready")]
    pub ready_timeout_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Remote {
    pub base_url: String,
    #[serde(default)]
    pub api_key_env: String,
    #[serde(default)]
    pub model: String,
}

// ────────────────────────── workflow ──────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct WorkflowManifest {
    #[serde(default = "api_ver")]
    pub api_version: String,
    pub workflow: WorkflowMeta,
    #[serde(default, rename = "step")]
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkflowMeta {
    pub name: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub about: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Step {
    #[serde(default)]
    pub id: String,
    /// 元素：PATH 上的命令名，或已注册的服务名
    #[serde(rename = "use")]
    pub use_: String,
    /// 有序参数（会被拼到元素后面）；支持 `{{in}}` `{{out}}` `{{outdir}}`
    #[serde(default)]
    pub args: Vec<String>,
    /// 把上一步的产物通过 **stdin** 喂给元素（适合 cat / tr / sed / grep 这类）
    #[serde(default)]
    pub pipe: bool,
    /// 结构化参数（保留，等价于 args 的另一种写法）
    #[serde(default)]
    pub with: Option<toml::Value>,
    /// 输入说明（自由文本，供人读；执行时以「上一步的产物」为准）
    #[serde(default, rename = "in")]
    pub in_: String,
    /// 产物文件名（落在本次运行的工作目录里）
    #[serde(default)]
    pub out: String,
}

// ────────────────────────── template ──────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct TemplateManifest {
    #[serde(default = "api_ver")]
    pub api_version: String,
    pub template: TemplateMeta,
    #[serde(default)]
    pub requires: Option<Requires>,
    #[serde(default)]
    pub defaults: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TemplateMeta {
    pub name: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub about: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Requires {
    #[serde(default)]
    pub capabilities: Vec<String>,
}

// ────────────────────────── model ──────────────────────────

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Catalog {
    #[serde(default, rename = "model")]
    pub models: Vec<ModelEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModelEntry {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub size: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub sha256: String,
}

// ────────────────────────── store ──────────────────────────

#[derive(Debug, Clone, Default)]
pub struct Store {
    pub root: PathBuf,
    pub services: Vec<ServiceManifest>,
    pub workflows: Vec<WorkflowManifest>,
    pub templates: Vec<TemplateManifest>,
    pub models: Vec<ModelEntry>,
}

impl Store {
    /// 扫目录加载全部 manifest（坏的单条跳过，不拖垮整体）
    pub fn load(root: &Path) -> Store {
        let mut s = Store {
            root: root.to_path_buf(),
            ..Default::default()
        };
        for dir in subdirs(&root.join("services")) {
            if let Some(m) = read_toml::<ServiceManifest>(&dir.join("service.toml")) {
                s.services.push(m);
            }
        }
        for dir in subdirs(&root.join("workflows")) {
            if let Some(m) = read_toml::<WorkflowManifest>(&dir.join("workflow.toml")) {
                s.workflows.push(m);
            }
        }
        for f in toml_files(&root.join("templates")) {
            if let Some(m) = read_toml::<TemplateManifest>(&f) {
                s.templates.push(m);
            }
        }
        if let Some(c) = read_toml::<Catalog>(&root.join("models/catalog.toml")) {
            s.models = c.models;
        }
        s.services.sort_by(|a, b| a.service.name.cmp(&b.service.name));
        s.workflows.sort_by(|a, b| a.workflow.name.cmp(&b.workflow.name));
        s
    }

    /// 校验（SPEC §8）：字段 / 唯一 / kind 词表 / 端口冲突 / 引用完整性
    pub fn validate(&self) -> Vec<String> {
        let mut errs = Vec::new();
        let mut names = HashSet::new();
        let mut ports: HashMap<u16, String> = HashMap::new();

        for s in &self.services {
            let m = &s.service;
            if !is_valid_name(&m.name) {
                errs.push(format!("服务名不合法（须小写字母开头，仅 a-z0-9-）: `{}`", m.name));
            }
            if !names.insert(m.name.clone()) {
                errs.push(format!("服务名重复: {}", m.name));
            }
            if !KINDS.contains(&m.kind.as_str()) {
                errs.push(format!("{}: 未知 kind `{}`（可选: {}）", m.name, m.kind, KINDS.join("/")));
            }
            match m.mode.as_str() {
                "local" => match &s.runtime {
                    None => errs.push(format!("{}: mode=local 必须有 [runtime]", m.name)),
                    Some(rt) => {
                        for addr in [&rt.listen, &rt.backend] {
                            match port_of(addr) {
                                None => errs.push(format!("{}: 地址须为 host:port（当前 `{}`）", m.name, addr)),
                                Some(p) => {
                                    if let Some(owner) = ports.insert(p, m.name.clone()) {
                                        errs.push(format!("端口 {} 冲突: `{}` 与 `{}`", p, owner, m.name));
                                    }
                                }
                            }
                        }
                    }
                },
                "remote" => {
                    if s.remote.is_none() {
                        errs.push(format!("{}: mode=remote 必须有 [remote]", m.name));
                    }
                }
                other => errs.push(format!("{}: mode 只能是 local/remote（当前 `{}`）", m.name, other)),
            }
        }

        // 工作流引用完整性
        let svc_names: HashSet<&str> = self.services.iter().map(|s| s.service.name.as_str()).collect();
        for w in &self.workflows {
            if w.steps.is_empty() {
                errs.push(format!("工作流 {}: 没有任何 [[step]]", w.workflow.name));
            }
            for st in &w.steps {
                if !KINDS.contains(&st.use_.as_str())
                    && !svc_names.contains(st.use_.as_str())
                    && !on_path(&st.use_)
                {
                    errs.push(format!(
                        "工作流 {}: 步骤 `{}` —— 元素 `{}` 既不是已知 kind/服务，也不在 PATH 上",
                        w.workflow.name, st.id, st.use_
                    ));
                }
            }
        }
        errs
    }

    /// 已用端口
    pub fn used_ports(&self) -> HashSet<u16> {
        self.services
            .iter()
            .filter_map(|s| s.runtime.as_ref())
            .flat_map(|r| [port_of(&r.listen), port_of(&r.backend)])
            .flatten()
            .collect()
    }

    /// 端口自动分配（SPEC §3.2）：扫最小空闲 —— 比盲目自增稳，删服务能回收
    pub fn alloc_ports(&self) -> Option<(u16, u16)> {
        let used = self.used_ports();
        let listen = (LISTEN_POOL.0..=LISTEN_POOL.1).find(|p| !used.contains(p))?;
        let backend = (BACKEND_POOL.0..=BACKEND_POOL.1).find(|p| !used.contains(p))?;
        Some((listen, backend))
    }

    pub fn service(&self, name: &str) -> Option<&ServiceManifest> {
        self.services.iter().find(|s| s.service.name == name)
    }

    pub fn service_names(&self) -> HashSet<String> {
        self.services.iter().map(|s| s.service.name.clone()).collect()
    }
}

// ────────────────────────── helpers ──────────────────────────

#[cfg(windows)]
const EXE_EXTS: &[&str] = &["", ".exe"];
#[cfg(not(windows))]
const EXE_EXTS: &[&str] = &[""];

/// 元素是否可用：带路径的直接看文件；否则在 PATH 上找（Windows 兼顾 `.exe`）
pub fn on_path(name: &str) -> bool {
    if name.trim().is_empty() {
        return false;
    }
    let p = Path::new(name);
    if p.components().count() > 1 {
        return p.is_file();
    }
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            for ext in EXE_EXTS {
                if dir.join(format!("{name}{ext}")).is_file() {
                    return true;
                }
            }
        }
    }
    false
}

pub fn port_of(addr: &str) -> Option<u16> {
    addr.rsplit_once(':').and_then(|(_, p)| p.parse().ok())
}

pub fn is_valid_name(n: &str) -> bool {
    let mut chars = n.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    n.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// 从 URL / 标题派生一个合法 name（不合法则回退 kind）
pub fn derive_name(url: Option<&str>, title: Option<&str>, kind: &str, existing: &HashSet<String>) -> String {
    let mut base = String::new();
    if let Some(u) = url {
        let host = u
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .split('/')
            .next()
            .unwrap_or("")
            .split(':')
            .next()
            .unwrap_or("");
        let skip = ["api", "www", "com", "cn", "io", "net", "org", "co", "v1", "chat", "open"];
        let cand = host
            .split('.')
            .find(|p| !skip.contains(&p.to_ascii_lowercase().as_str()))
            .unwrap_or("");
        base = slug(cand);
    }
    if base.is_empty() {
        if let Some(t) = title {
            base = slug(t);
        }
    }
    if base.is_empty() || !base.chars().next().map(|c| c.is_ascii_lowercase()).unwrap_or(false) {
        base = kind.to_string();
    }
    let mut name = base.clone();
    let mut i = 2;
    while existing.contains(&name) {
        name = format!("{base}-{i}");
        i += 1;
    }
    name
}

fn slug(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    cleaned.trim_matches('-').to_string()
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

fn toml_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_file() && p.extension().map(|x| x == "toml").unwrap_or(false) {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

fn read_toml<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    let text = std::fs::read_to_string(path).ok()?;
    toml::from_str(&text).ok()
}

/// `init` 用：示例服务清单
pub const EXAMPLE_SERVICE: &str = r#"api_version = "lyco/v1"

[service]
name  = "example"
kind  = "llm"
mode  = "remote"
title = "示例服务"

[remote]
base_url    = "https://api.example.com/v1"
api_key_env = "LYCO_EXAMPLE_KEY"
model       = "gpt-4o-mini"
"#;

/// `init` 用：示例工作流
///
/// **元素 = PATH 上的一个命令**；**工作流 = 有序步骤，上一步的产物 = 下一步的输入**。
pub const EXAMPLE_WORKFLOW: &str = r#"api_version = "lyco/v1"

[workflow]
name  = "video2subtitle"
title = "视频转字幕"
about = "视频 → 抽音 → 转写 → SRT"

[[step]]
id   = "audio"
use  = "ffmpeg"
args = ["-y", "-i", "{{in}}", "-vn", "-ac", "1", "-ar", "16000", "{{out}}"]
out  = "audio.wav"

[[step]]
id   = "asr"
use  = "whisper"
args = ["-m", "/opt/lyco/models/ggml-base.bin", "-f", "{{in}}", "-osrt", "-of", "{{outbase}}"]
out  = "asr.srt"

[[step]]
id   = "clean"
use  = "cat"
args = ["{{in}}"]
out  = "final.srt"
"#;

// ────────────────────────── tests ──────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_parsing() {
        assert_eq!(port_of("0.0.0.0:7300"), Some(7300));
        assert_eq!(port_of("127.0.0.1:7400"), Some(7400));
        assert_eq!(port_of("nonsense"), None);
    }

    #[test]
    fn name_validation() {
        assert!(is_valid_name("rembg"));
        assert!(is_valid_name("edge-tts2"));
        assert!(!is_valid_name("Rembg"));
        assert!(!is_valid_name("2x"));
        assert!(!is_valid_name("-x"));
    }

    #[test]
    fn derive_from_url() {
        let e = HashSet::new();
        assert_eq!(
            derive_name(Some("https://api.deepseek.com/v1"), None, "llm", &e),
            "deepseek"
        );
        assert_eq!(derive_name(Some("https://api.x.com/v1"), None, "llm", &e), "x");
    }

    #[test]
    fn derive_falls_back_to_kind_for_cjk() {
        let e = HashSet::new();
        assert_eq!(derive_name(None, Some("抠图"), "rembg", &e), "rembg");
    }

    #[test]
    fn derive_dedups() {
        let mut e = HashSet::new();
        e.insert("deepseek".to_string());
        assert_eq!(
            derive_name(Some("https://api.deepseek.com/v1"), None, "llm", &e),
            "deepseek-2"
        );
    }

    fn svc(name: &str, listen: u16, backend: u16) -> ServiceManifest {
        ServiceManifest {
            api_version: API_VERSION.into(),
            service: ServiceMeta {
                name: name.into(),
                kind: "custom".into(),
                mode: "local".into(),
                title: String::new(),
                about: String::new(),
            },
            runtime: Some(Runtime {
                listen: format!("0.0.0.0:{listen}"),
                backend: format!("127.0.0.1:{backend}"),
                up: String::new(),
                down: String::new(),
                idle_timeout_secs: 300,
                ready_timeout_secs: 30,
            }),
            remote: None,
            params: None,
        }
    }

    #[test]
    fn alloc_skips_used_and_recycles() {
        let mut store = Store::default();
        store.services.push(svc("a", 7300, 7400));
        assert_eq!(store.alloc_ports(), Some((7301, 7401)));
        // 删掉 a 后端口应能回收
        store.services.clear();
        assert_eq!(store.alloc_ports(), Some((7300, 7400)));
    }

    #[test]
    fn validate_catches_conflicts_and_bad_kind() {
        let mut store = Store::default();
        let mut a = svc("a", 7300, 7400);
        a.service.kind = "nonsense".into();
        store.services.push(a);
        store.services.push(svc("b", 7300, 7400));
        let errs = store.validate();
        assert!(errs.iter().any(|e| e.contains("未知 kind")), "{errs:?}");
        assert!(errs.iter().any(|e| e.contains("端口 7300 冲突")), "{errs:?}");
    }

    #[test]
    fn validate_catches_broken_workflow_ref() {
        let mut store = Store::default();
        store.workflows.push(WorkflowManifest {
            api_version: API_VERSION.into(),
            workflow: WorkflowMeta {
                name: "w".into(),
                title: String::new(),
                about: String::new(),
            },
            steps: vec![Step {
                id: "s".into(),
                use_: "nope".into(),
                args: vec![],
                pipe: false,
                with: None,
                in_: String::new(),
                out: String::new(),
            }],
        });
        let errs = store.validate();
        assert!(errs.iter().any(|e| e.contains("不在 PATH 上")), "{errs:?}");
    }
}
