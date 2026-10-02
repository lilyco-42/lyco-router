//! lyco-router v0.2 —— 按需服务路由器（能力 + 工作流）
//!
//! ```bash
//! lyco-router init                                    # 建配置目录骨架
//! lyco-router add --kind llm --url https://api.x.com/v1 --key sk-x   # 注册远程 API
//! lyco-router add --kind rembg --title 抠图            # 注册本地引擎（端口自动）
//! lyco-router list                                    # 列出服务 + 工作流
//! lyco-router check                                   # 校验全部 manifest
//! lyco-router serve                                   # 跑守护进程
//! lyco-router --mcp                                   # 暴露为 MCP 服务器
//! ```

mod api;
mod config;
mod daemon;
mod scaffold;

use std::path::Path;
use std::time::Instant;

use lilyco::prelude::*;

const DEFAULT_ROOT: &str = "/etc/lyco-router";

// ────────────────────────────── add ──────────────────────────────

/// 注册一个服务（有 --url 则为远程 API，否则为本地引擎）
#[derive(App)]
#[app(name = "add", about = "注册一个服务（有 --url → 远程 API；否则 → 本地引擎，端口自动分配）", run = "run_add")]
struct AddCmd {
    /// 服务类型
    #[arg(about = "类型: asr/tts/ocr/rembg/llm/download/media/custom")]
    kind: String,
    /// API 地址（填了即注册为远程 API 服务）
    #[arg(about = "API 地址（填了即 remote；留空即 local）")]
    url: Option<String>,
    /// 服务名（默认从 url 派生）
    #[arg(about = "服务名（默认从 url 派生）")]
    name: Option<String>,
    /// 显示名
    #[arg(about = "显示名（卡片标题）")]
    title: Option<String>,
    /// API 密钥（仅提示存环境变量，不写文件）
    #[arg(about = "API 密钥（只提示存环境变量，不写文件）")]
    key: Option<String>,
    /// 默认模型
    #[arg(about = "默认模型")]
    model: Option<String>,
    /// 配置根目录
    #[arg(about = "配置根目录", default = "/etc/lyco-router")]
    root: String,
}

fn run_add(app: &AddCmd, ctx: &Context) -> Result<serde_json::Value, AppError> {
    let opt = scaffold::AddOpt {
        kind: app.kind.clone(),
        name: app.name.clone(),
        title: app.title.clone(),
        url: app.url.clone(),
        model: app.model.clone(),
        root: Path::new(&app.root).to_path_buf(),
    };
    let r = scaffold::add_service(&opt).map_err(AppError::Runtime)?;
    ctx.log(LogLevel::Info, format!("已注册 `{}`（{}）→ {}", r.name, r.mode, r.path.display()));
    if let Some((l, b)) = r.ports {
        ctx.log(LogLevel::Info, format!("端口自动分配: listen {l} / backend {b}"));
    }
    if let Some(env) = &r.key_env {
        if app.key.is_some() {
            ctx.log(LogLevel::Warn, "出于安全，--key 未写入 manifest");
        }
        ctx.log(LogLevel::Info, format!("密钥请存环境变量: export {env}=<你的key>"));
    }
    let result = serde_json::json!({
        "name": r.name,
        "mode": r.mode,
        "path": r.path.display().to_string(),
        "listen": r.ports.map(|(l, _)| l),
        "backend": r.ports.map(|(_, b)| b),
        "key_env": r.key_env,
    });
    ctx.done(result.clone(), 0);
    Ok(result)
}

// ────────────────────────────── list ──────────────────────────────

/// 列出所有服务与工作流
#[derive(App)]
#[app(name = "list", about = "列出所有服务与工作流", run = "run_list")]
struct ListCmd {
    /// 配置根目录
    #[arg(about = "配置根目录", default = "/etc/lyco-router")]
    root: String,
}

fn run_list(app: &ListCmd, ctx: &Context) -> Result<serde_json::Value, AppError> {
    let store = config::Store::load(Path::new(&app.root));
    let mut svcs = Vec::new();
    for s in &store.services {
        let m = &s.service;
        let state = match m.mode.as_str() {
            "local" => match &s.runtime {
                Some(rt) => {
                    if daemon::backend_alive(&rt.backend) {
                        "up"
                    } else {
                        "down"
                    }
                }
                None => "invalid",
            },
            _ => "remote",
        };
        ctx.log(
            LogLevel::Info,
            format!("{:<16} {:<8} {:<7} {}", m.name, m.kind, m.mode, state),
        );
        svcs.push(serde_json::json!({
            "name": m.name, "kind": m.kind, "mode": m.mode, "title": m.title, "state": state,
        }));
    }
    let mut wfs = Vec::new();
    for w in &store.workflows {
        ctx.log(
            LogLevel::Info,
            format!("[workflow] {:<14} {} 步", w.workflow.name, w.steps.len()),
        );
        wfs.push(serde_json::json!({
            "name": w.workflow.name, "title": w.workflow.title, "steps": w.steps.len(),
        }));
    }
    let result = serde_json::json!({
        "services": svcs,
        "workflows": wfs,
        "templates": store.templates.iter().map(|t| t.template.name.clone()).collect::<Vec<_>>(),
        "models": store.models.len(),
    });
    ctx.done(result.clone(), 0);
    Ok(result)
}

// ────────────────────────────── check ──────────────────────────────

/// 校验整个配置目录
#[derive(App)]
#[app(name = "check", about = "校验整个配置目录（字段/唯一/端口冲突/引用完整性）", run = "run_check")]
struct CheckCmd {
    /// 配置根目录
    #[arg(about = "配置根目录", default = "/etc/lyco-router")]
    root: String,
}

fn run_check(app: &CheckCmd, ctx: &Context) -> Result<serde_json::Value, AppError> {
    let store = config::Store::load(Path::new(&app.root));
    let errs = store.validate();
    for e in &errs {
        ctx.log(LogLevel::Error, e.clone());
    }
    ctx.log(
        LogLevel::Info,
        format!(
            "服务 {} · 工作流 {} · 模板 {} · 模型 {}",
            store.services.len(),
            store.workflows.len(),
            store.templates.len(),
            store.models.len()
        ),
    );
    let result = serde_json::json!({
        "ok": errs.is_empty(),
        "errors": errs,
        "services": store.services.len(),
        "workflows": store.workflows.len(),
    });
    ctx.done(result.clone(), 0);
    Ok(result)
}

// ────────────────────────────── status ──────────────────────────────

/// 查看单个服务的详情
#[derive(App)]
#[app(name = "status", about = "查看某个服务的详情", run = "run_status")]
struct StatusCmd {
    /// 服务名
    #[arg(about = "服务名")]
    name: String,
    /// 配置根目录
    #[arg(about = "配置根目录", default = "/etc/lyco-router")]
    root: String,
}

fn run_status(app: &StatusCmd, ctx: &Context) -> Result<serde_json::Value, AppError> {
    let store = config::Store::load(Path::new(&app.root));
    let s = store
        .service(&app.name)
        .ok_or_else(|| AppError::InvalidArg(format!("没有服务 `{}`", app.name)))?;
    let m = &s.service;
    let (state, listen, backend) = match &s.runtime {
        Some(rt) => (
            (if daemon::backend_alive(&rt.backend) { "up" } else { "down" }).to_string(),
            rt.listen.clone(),
            rt.backend.clone(),
        ),
        None => ("remote".to_string(), String::new(), String::new()),
    };
    let result = serde_json::json!({
        "name": m.name, "kind": m.kind, "mode": m.mode, "title": m.title,
        "state": state, "listen": listen, "backend": backend,
    });
    ctx.done(result.clone(), 0);
    Ok(result)
}

// ────────────────────────────── up ──────────────────────────────

/// 手动拉起某个 local 服务的后端
#[derive(App)]
#[app(name = "up", about = "手动启动某个 local 服务的后端并等待就绪", run = "run_up")]
struct UpCmd {
    /// 服务名
    #[arg(about = "服务名")]
    name: String,
    /// 配置根目录
    #[arg(about = "配置根目录", default = "/etc/lyco-router")]
    root: String,
}

fn run_up(app: &UpCmd, ctx: &Context) -> Result<serde_json::Value, AppError> {
    let start = Instant::now();
    let store = config::Store::load(Path::new(&app.root));
    let s = store
        .service(&app.name)
        .ok_or_else(|| AppError::InvalidArg(format!("没有服务 `{}`", app.name)))?;
    let rt = s
        .runtime
        .as_ref()
        .ok_or_else(|| AppError::InvalidArg(format!("`{}` 是 remote 服务，无需启动", app.name)))?;
    if daemon::backend_alive(&rt.backend) {
        let r = serde_json::json!({"name": app.name, "action": "up", "ready": true, "note": "已在运行"});
        ctx.done(r.clone(), start.elapsed().as_millis() as u64);
        return Ok(r);
    }
    ctx.emit(Progress::Started { total: Some(1), message: Some(format!("启动 {}", app.name)) });
    daemon::run_cmd(&rt.up);
    let ready = daemon::wait_ready_blocking(&rt.backend, rt.ready_timeout_secs);
    let r = serde_json::json!({"name": app.name, "action": "up", "ready": ready});
    ctx.done(r.clone(), start.elapsed().as_millis() as u64);
    Ok(r)
}

// ────────────────────────────── down ──────────────────────────────

/// 手动停掉某个 local 服务的后端
#[derive(App)]
#[app(name = "down", about = "手动停止某个 local 服务的后端", run = "run_down")]
struct DownCmd {
    /// 服务名
    #[arg(about = "服务名")]
    name: String,
    /// 配置根目录
    #[arg(about = "配置根目录", default = "/etc/lyco-router")]
    root: String,
}

fn run_down(app: &DownCmd, ctx: &Context) -> Result<serde_json::Value, AppError> {
    let store = config::Store::load(Path::new(&app.root));
    let s = store
        .service(&app.name)
        .ok_or_else(|| AppError::InvalidArg(format!("没有服务 `{}`", app.name)))?;
    let rt = s
        .runtime
        .as_ref()
        .ok_or_else(|| AppError::InvalidArg(format!("`{}` 是 remote 服务，无需停止", app.name)))?;
    daemon::run_cmd(&rt.down);
    let r = serde_json::json!({"name": app.name, "action": "down"});
    ctx.done(r.clone(), 0);
    Ok(r)
}

// ────────────────────────────── serve ──────────────────────────────

/// 运行按需路由器守护进程（前台）
#[derive(App)]
#[app(name = "serve", about = "运行按需路由器守护进程（前台常驻）", run = "run_serve")]
struct ServeCmd {
    /// 配置根目录
    #[arg(about = "配置根目录", default = "/etc/lyco-router")]
    root: String,
    /// 空闲回收扫描间隔（秒）
    #[arg(about = "空闲回收扫描间隔（秒）", default = 5)]
    idle_check_secs: u64,
    /// 管理页 / JSON API 端口（0 = 关闭）
    #[arg(about = "管理页 / JSON API 端口（0 = 关闭）", default = 8080)]
    port: u16,
}

fn run_serve(app: &ServeCmd, ctx: &Context) -> Result<serde_json::Value, AppError> {
    let store = config::Store::load(Path::new(&app.root));
    let errs = store.validate();
    if !errs.is_empty() {
        return Err(AppError::Runtime(format!(
            "配置有 {} 处问题，先跑 check：{}",
            errs.len(),
            errs[0]
        )));
    }
    let targets: Vec<daemon::Target> = store
        .services
        .iter()
        .filter(|s| s.service.mode == "local")
        .filter_map(|s| {
            s.runtime.clone().map(|rt| daemon::Target {
                name: s.service.name.clone(),
                rt,
            })
        })
        .collect();
    if targets.is_empty() {
        ctx.log(LogLevel::Warn, "没有 local 服务可托管，只跑管理页 / API");
    }
    // 管理页 / JSON API（小白入口）
    if app.port > 0 {
        let p = api::spawn(store.root.clone(), app.port)
            .map_err(|e| AppError::Runtime(format!("管理页启动失败: {e}")))?;
        ctx.log(LogLevel::Info, format!("管理页: http://<板子IP>:{p}/  （JSON API: /api/services）"));
    }
    ctx.emit(Progress::Started {
        total: None,
        message: Some(format!("按需路由器启动，{} 个 local 服务", targets.len())),
    });
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| AppError::Runtime(format!("tokio runtime: {e}")))?;
    rt.block_on(daemon::serve(targets, app.idle_check_secs))
        .map_err(|e| AppError::Runtime(e.to_string()))?;
    Ok(serde_json::json!({"ok": true}))
}

// ────────────────────────────── init ──────────────────────────────

/// 建立配置目录骨架（含示例）
#[derive(App)]
#[app(name = "init", about = "建立配置目录骨架（services/workflows/templates/models + 示例）", run = "run_init")]
struct InitCmd {
    /// 配置根目录
    #[arg(about = "配置根目录", default = "/etc/lyco-router")]
    root: String,
    /// 覆盖已有文件
    #[arg(about = "覆盖已存在的文件")]
    force: bool,
}

fn run_init(app: &InitCmd, ctx: &Context) -> Result<serde_json::Value, AppError> {
    let made = scaffold::init_tree(Path::new(&app.root), app.force).map_err(AppError::Runtime)?;
    for m in &made {
        ctx.log(LogLevel::Info, format!("写出 {m}"));
    }
    let r = serde_json::json!({"ok": true, "root": app.root, "created": made});
    ctx.done(r.clone(), 0);
    Ok(r)
}

// ────────────────────────────── main ──────────────────────────────

fn build_registry() -> Registry {
    let mut registry = Registry::new();
    registry.register(RegisteredCommand::from_app::<AddCmd>()).unwrap();
    registry.register(RegisteredCommand::from_app::<ListCmd>()).unwrap();
    registry.register(RegisteredCommand::from_app::<CheckCmd>()).unwrap();
    registry.register(RegisteredCommand::from_app::<StatusCmd>()).unwrap();
    registry.register(RegisteredCommand::from_app::<UpCmd>()).unwrap();
    registry.register(RegisteredCommand::from_app::<DownCmd>()).unwrap();
    registry.register(RegisteredCommand::from_app::<ServeCmd>()).unwrap();
    registry.register(RegisteredCommand::from_app::<InitCmd>()).unwrap();
    registry
}

fn main() {
    let _ = DEFAULT_ROOT;
    let registry = build_registry();
    if std::env::args().any(|a| a == "--mcp") {
        lilyco::serve_mcp(registry);
    } else {
        lilyco::run_cli_registry("lyco-router", registry);
    }
}
