//! scaffold：`add` 命令 —— 从最少输入生成整份 manifest（Rails `generate` 思路）。
//!
//! - 有 `--url` → 生成 `mode = "remote"`（不占端口、无起停）
//! - 无 `--url` → 生成 `mode = "local"`（自动分配端口 + up/down 骨架）
//! - `name` 从域名派生；端口扫最小空闲；其余全默认值。

use std::path::PathBuf;

use crate::config::{derive_name, is_valid_name, Store, API_VERSION, KINDS};

pub struct AddOpt {
    pub kind: String,
    pub name: Option<String>,
    pub title: Option<String>,
    pub url: Option<String>,
    pub model: Option<String>,
    pub root: PathBuf,
}

pub struct AddResult {
    pub name: String,
    pub mode: String,
    pub path: PathBuf,
    pub ports: Option<(u16, u16)>,
    pub key_env: Option<String>,
}

pub fn add_service(opt: &AddOpt) -> Result<AddResult, String> {
    if !KINDS.contains(&opt.kind.as_str()) {
        return Err(format!(
            "未知类型 `{}`（可选: {}）",
            opt.kind,
            KINDS.join(" / ")
        ));
    }

    let store = Store::load(&opt.root);
    let existing = store.service_names();

    let name = match opt.name.as_deref() {
        Some(n) if !n.is_empty() => {
            if !is_valid_name(n) {
                return Err(format!("名字不合法（须小写字母开头，仅 a-z0-9-）: `{n}`"));
            }
            if existing.contains(n) {
                return Err(format!("服务 `{n}` 已存在"));
            }
            n.to_string()
        }
        _ => derive_name(
            opt.url.as_deref(),
            opt.title.as_deref(),
            &opt.kind,
            &existing,
        ),
    };

    let title = opt
        .title
        .clone()
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| name.clone());
    let kind = &opt.kind;

    let dir = opt.root.join("services").join(&name);
    std::fs::create_dir_all(&dir).map_err(|e| format!("建目录 {} 失败: {e}", dir.display()))?;
    let path = dir.join("service.toml");

    if let Some(url) = opt.url.as_deref().filter(|u| !u.is_empty()) {
        // ── remote：某个 AI 网站的 API ──
        let key_env = format!("LYCO_{}_KEY", name.to_uppercase().replace('-', "_"));
        let model = opt.model.clone().unwrap_or_default();
        let text = format!(
            "api_version = \"{API_VERSION}\"\n\n\
             [service]\n\
             name  = \"{name}\"\n\
             kind  = \"{kind}\"\n\
             mode  = \"remote\"\n\
             title = \"{title}\"\n\n\
             [remote]\n\
             base_url    = \"{url}\"\n\
             api_key_env = \"{key_env}\"\n\
             model       = \"{model}\"\n"
        );
        std::fs::write(&path, text).map_err(|e| format!("写文件失败: {e}"))?;
        Ok(AddResult {
            name,
            mode: "remote".into(),
            path,
            ports: None,
            key_env: Some(key_env),
        })
    } else {
        // ── local：本地引擎 ──
        let (listen, backend) = store.alloc_ports().ok_or("端口池已满（7300-7399 / 7400-7499）")?;
        let text = format!(
            "api_version = \"{API_VERSION}\"\n\n\
             [service]\n\
             name  = \"{name}\"\n\
             kind  = \"{kind}\"\n\
             mode  = \"local\"\n\
             title = \"{title}\"\n\n\
             [runtime]\n\
             listen  = \"0.0.0.0:{listen}\"\n\
             backend = \"127.0.0.1:{backend}\"\n\
             up      = \"systemctl start lyco-{name}\"\n\
             down    = \"systemctl stop lyco-{name}\"\n\
             idle_timeout_secs  = 300\n\
             ready_timeout_secs = 30\n"
        );
        std::fs::write(&path, text).map_err(|e| format!("写文件失败: {e}"))?;
        Ok(AddResult {
            name,
            mode: "local".into(),
            path,
            ports: Some((listen, backend)),
            key_env: None,
        })
    }
}

/// `init`：建目录骨架
pub fn init_tree(root: &std::path::Path, force: bool) -> Result<Vec<String>, String> {
    use crate::config::{EXAMPLE_SERVICE, EXAMPLE_WORKFLOW};
    let mut made = Vec::new();
    for d in ["services", "workflows", "templates", "models"] {
        std::fs::create_dir_all(root.join(d)).map_err(|e| format!("建 {d} 失败: {e}"))?;
    }
    let svc = root.join("services/example/service.toml");
    if force || !svc.exists() {
        std::fs::create_dir_all(svc.parent().unwrap()).ok();
        std::fs::write(&svc, EXAMPLE_SERVICE).map_err(|e| e.to_string())?;
        made.push(svc.display().to_string());
    }
    let wf = root.join("workflows/video2subtitle/workflow.toml");
    if force || !wf.exists() {
        std::fs::create_dir_all(wf.parent().unwrap()).ok();
        std::fs::write(&wf, EXAMPLE_WORKFLOW).map_err(|e| e.to_string())?;
        made.push(wf.display().to_string());
    }
    Ok(made)
}
