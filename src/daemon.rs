//! 按需路由器核心：每个 local 服务一个前端监听，有人连就唤醒后端并代理，空闲即停。
//!
//! 思路同 `itzg/mc-router`：常驻的只是这层薄薄的路由器（几十 MB 内存），
//! 真正的重服务（rembg / whisper / galgame …）平时完全不在跑。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

use crate::config::Runtime;

/// 一个要按需托管的 local 服务
#[derive(Debug, Clone)]
pub struct Target {
    pub name: String,
    pub rt: Runtime,
}

fn log(msg: &str) {
    println!("[{}] {}", now_hms(), msg);
}

fn now_hms() -> String {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let s = d.as_secs();
    format!("{:02}:{:02}:{:02}", (s / 3600) % 24, (s / 60) % 60, s % 60)
}

/// 执行 shell 命令（起停后端）；失败只记录，不 panic。
pub fn run_cmd(cmd: &str) {
    if cmd.trim().is_empty() {
        return;
    }
    match std::process::Command::new("sh").arg("-c").arg(cmd).status() {
        Ok(st) if st.success() => {}
        Ok(st) => log(&format!("命令退出码 {:?}: {}", st.code(), cmd)),
        Err(e) => log(&format!("命令执行失败 {e}: {cmd}")),
    }
}

/// 同步探测后端端口是否在监听（list / status 用）
pub fn backend_alive(addr: &str) -> bool {
    use std::net::{TcpStream as StdStream, ToSocketAddrs};
    let Ok(mut it) = addr.to_socket_addrs() else {
        return false;
    };
    match it.next() {
        Some(sa) => StdStream::connect_timeout(&sa, Duration::from_millis(500)).is_ok(),
        None => false,
    }
}

/// 同步等待后端就绪（up 命令用）
pub fn wait_ready_blocking(addr: &str, timeout_secs: u64) -> bool {
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        if backend_alive(addr) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(300));
    }
}

struct St {
    up: bool,
    last: Instant,
}

/// 守护进程入口：为每个 local 服务起监听 + 一个空闲回收循环。永久运行。
pub async fn serve(targets: Vec<Target>, idle_check_secs: u64) -> anyhow::Result<()> {
    let states: Arc<Mutex<HashMap<String, St>>> = Arc::new(Mutex::new(HashMap::new()));
    for t in &targets {
        states
            .lock()
            .await
            .insert(t.name.clone(), St { up: false, last: Instant::now() });
    }

    // 空闲回收
    {
        let states = states.clone();
        let ts = targets.clone();
        let interval = idle_check_secs.max(1);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(interval)).await;
                let mut st = states.lock().await;
                for t in &ts {
                    if let Some(e) = st.get_mut(&t.name) {
                        if e.up && e.last.elapsed().as_secs() >= t.rt.idle_timeout_secs {
                            log(&format!(
                                "[{}] 空闲 {}s ≥ 阈值，执行 down",
                                t.name, t.rt.idle_timeout_secs
                            ));
                            run_cmd(&t.rt.down);
                            e.up = false;
                        }
                    }
                }
            }
        });
    }

    let mut tasks = Vec::new();
    for t in targets {
        let states = states.clone();
        tasks.push(tokio::spawn(async move {
            if let Err(e) = serve_one(t, states).await {
                log(&format!("监听线程出错: {e}"));
            }
        }));
    }
    for task in tasks {
        let _ = task.await;
    }
    Ok(())
}

async fn serve_one(t: Target, states: Arc<Mutex<HashMap<String, St>>>) -> anyhow::Result<()> {
    let listener = TcpListener::bind(&t.rt.listen).await?;
    log(&format!(
        "[{}] 监听 {} -> {}（空闲 {}s 停）",
        t.name, t.rt.listen, t.rt.backend, t.rt.idle_timeout_secs
    ));
    loop {
        let (mut inbound, peer) = listener.accept().await?;
        let t2 = t.clone();
        let st = states.clone();
        tokio::spawn(async move {
            log(&format!("[{}] {peer} 接入", t2.name));

            let need_up = {
                let mut g = st.lock().await;
                match g.get_mut(&t2.name) {
                    Some(e) => {
                        e.last = Instant::now();
                        !e.up
                    }
                    None => true,
                }
            };
            if need_up {
                log(&format!("[{}] 唤醒后端：{}", t2.name, t2.rt.up));
                run_cmd(&t2.rt.up);
                if !wait_ready(&t2.rt.backend, t2.rt.ready_timeout_secs).await {
                    log(&format!(
                        "[{}] 后端 {}s 内未就绪，放弃本次连接",
                        t2.name, t2.rt.ready_timeout_secs
                    ));
                    return;
                }
                let mut g = st.lock().await;
                if let Some(e) = g.get_mut(&t2.name) {
                    e.up = true;
                    e.last = Instant::now();
                }
                log(&format!("[{}] 后端就绪", t2.name));
            }

            match TcpStream::connect(&t2.rt.backend).await {
                Ok(mut backend) => {
                    let _ = copy_bidirectional(&mut inbound, &mut backend).await;
                }
                Err(e) => log(&format!("[{}] 连接后端 {} 失败: {e}", t2.name, t2.rt.backend)),
            }
            let mut g = st.lock().await;
            if let Some(e) = g.get_mut(&t2.name) {
                e.last = Instant::now();
            }
            log(&format!("[{}] {peer} 断开", t2.name));
        });
    }
}

async fn wait_ready(addr: &str, timeout_secs: u64) -> bool {
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        if TcpStream::connect(addr).await.is_ok() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
}
