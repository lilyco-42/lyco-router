//! 管理页 + JSON API —— 小白唯一的入口。
//!
//! 铁律：**业务规则全在后端**（scaffold / config），前端只是 renderer。
//! 端口分配、校验、manifest 生成都发生在 Rust 侧，换掉前端不影响后端。

use std::path::PathBuf;

use serde::Deserialize;
use tiny_http::{Header, Method, Request, Response, Server};

use crate::config::Store;
use crate::scaffold::{self, AddOpt};

const INDEX: &str = include_str!("web/index.html");

/// 在后台线程起 HTTP 服务；返回实际绑定端口。
pub fn spawn(root: PathBuf, port: u16) -> std::io::Result<u16> {
    let server = Server::http(("0.0.0.0", port))
        .map_err(|e| std::io::Error::other(format!("bind 0.0.0.0:{port} 失败: {e}")))?;
    let actual = server
        .server_addr()
        .to_ip()
        .map(|a| a.port())
        .unwrap_or(port);
    std::thread::spawn(move || {
        for req in server.incoming_requests() {
            handle(&root, req);
        }
    });
    Ok(actual)
}

fn hdr(name: &[u8], val: &[u8]) -> Header {
    Header::from_bytes(name, val).expect("static header")
}

fn cors() -> Vec<Header> {
    vec![
        hdr(b"Access-Control-Allow-Origin", b"*"),
        hdr(b"Access-Control-Allow-Methods", b"GET,POST,OPTIONS"),
        hdr(b"Access-Control-Allow-Headers", b"Content-Type"),
    ]
}

fn send(req: Request, code: u16, ctype: &str, body: String) {
    let mut resp = Response::from_string(body).with_status_code(code);
    resp = resp.with_header(hdr(b"Content-Type", ctype.as_bytes()));
    for h in cors() {
        resp = resp.with_header(h);
    }
    let _ = req.respond(resp);
}

fn send_json(req: Request, code: u16, v: serde_json::Value) {
    send(req, code, "application/json; charset=utf-8", v.to_string());
}

#[derive(Deserialize)]
struct AddReq {
    kind: String,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    key: Option<String>,
}

fn handle(root: &PathBuf, mut req: Request) {
    let method = req.method().clone();
    let url = req.url().split('?').next().unwrap_or("").to_string();

    if method == Method::Options {
        let mut resp = Response::empty(204);
        for h in cors() {
            resp = resp.with_header(h);
        }
        let _ = req.respond(resp);
        return;
    }

    match (method, url.as_str()) {
        (Method::Get, "/") | (Method::Get, "/index.html") => {
            send(req, 200, "text/html; charset=utf-8", INDEX.to_string());
        }
        (Method::Get, "/api/health") => send_json(
            req,
            200,
            serde_json::json!({"ok": true, "api_version": crate::config::API_VERSION}),
        ),
        (Method::Get, "/api/services") => {
            let store = Store::load(root);
            let items: Vec<_> = store
                .services
                .iter()
                .map(|s| {
                    let m = &s.service;
                    let state = match (&s.runtime, m.mode.as_str()) {
                        (Some(rt), "local") => {
                            if crate::daemon::backend_alive(&rt.backend) {
                                "up"
                            } else {
                                "down"
                            }
                        }
                        _ => "remote",
                    };
                    serde_json::json!({
                        "name": m.name, "kind": m.kind, "mode": m.mode,
                        "title": m.title, "about": m.about, "state": state,
                    })
                })
                .collect();
            send_json(
                req,
                200,
                serde_json::json!({"services": items, "count": items.len()}),
            );
        }
        (Method::Post, "/api/services") => {
            let mut body = String::new();
            if req.as_reader().read_to_string(&mut body).is_err() {
                send_json(req, 400, serde_json::json!({"ok": false, "error": "读取请求体失败"}));
                return;
            }
            let parsed: AddReq = match serde_json::from_str(&body) {
                Ok(v) => v,
                Err(e) => {
                    send_json(req, 400, serde_json::json!({"ok": false, "error": format!("JSON 解析失败: {e}")}));
                    return;
                }
            };
            let key_given = parsed.key.as_deref().map(|s| !s.is_empty()).unwrap_or(false);
            let opt = AddOpt {
                kind: parsed.kind,
                name: parsed.name.filter(|s| !s.is_empty()),
                title: parsed.title.filter(|s| !s.is_empty()),
                url: parsed.url.filter(|s| !s.is_empty()),
                model: parsed.model.filter(|s| !s.is_empty()),
                root: root.clone(),
            };
            match scaffold::add_service(&opt) {
                Ok(r) => send_json(
                    req,
                    200,
                    serde_json::json!({
                        "ok": true, "name": r.name, "mode": r.mode,
                        "path": r.path.display().to_string(),
                        "listen": r.ports.map(|(l, _)| l),
                        "backend": r.ports.map(|(_, b)| b),
                        "key_env": r.key_env,
                        "key_provided": key_given,
                    }),
                ),
                Err(e) => send_json(req, 400, serde_json::json!({"ok": false, "error": e})),
            }
        }
        (Method::Get, "/api/workflows") => {
            let store = Store::load(root);
            let items: Vec<_> = store
                .workflows
                .iter()
                .map(|w| {
                    serde_json::json!({
                        "name": w.workflow.name, "title": w.workflow.title, "about": w.workflow.about,
                        "steps": w.steps.iter()
                            .map(|s| serde_json::json!({"id": s.id, "use": s.use_}))
                            .collect::<Vec<_>>(),
                    })
                })
                .collect();
            send_json(
                req,
                200,
                serde_json::json!({"workflows": items, "count": items.len()}),
            );
        }
        _ => send_json(req, 404, serde_json::json!({"ok": false, "error": "not found"})),
    }
}
