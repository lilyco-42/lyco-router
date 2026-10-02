# 开发者接入指南

> 目标：**你的工具（不管什么形态）怎么接进 lyco**。看完这一页你就能自己接。
> 规范细节见 [SPEC.md](../SPEC.md)（`lyco/v1`）。

---

## 第 0 步：先判断你的是哪一类

**lyco 只认「服务」和「工作流」两种东西。** 你的工具先归类：

| 你的东西 | 特征 | 接入形态 | 工作量 |
|---|---|---|---|
| **常驻服务** | 有端口，一直在跑（HTTP/TCP） | `local` 服务 | 10 分钟 |
| **远程 API** | 别人的 URL + key | `remote` 服务 | 2 分钟 |
| **CLI 工具** | 跑完就退（`brush`、`ffmpeg`、脚本） | ① 包一层 HTTP → `local`；② 或做**工作流步骤** | 半小时 |
| **给 Agent 用的工具** | DSH / Claude / Cursor 调 | 用 **lilyco** 写 → 天生出 **MCP** | 看代码量 |

> **判断口诀**：能回答「怎么起来 / 怎么停 / 在哪监听」→ 就是 `local` 服务；
> 答不上来（CLI）→ 先包一层，或者干脆当工作流里的一步。

---

## 模板 A：常驻服务（最直接）

**1. 写 systemd 单元** `/etc/systemd/system/lyco-mytool.service`

```ini
[Unit]
Description=我的工具（lyco-router 按需拉起）
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/mytool --listen 127.0.0.1:7410
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

⚠️ **不要 `systemctl enable`** —— 它由 lyco-router 按需 `start`。

**2. 写 manifest** `/etc/lyco-router/services/mytool/service.toml`

```toml
api_version = "lyco/v1"

[service]
name  = "mytool"
kind  = "custom"          # 见 SPEC §4 词表
mode  = "local"
title = "我的工具"

[runtime]
listen  = "0.0.0.0:7310"      # 前端（lyco-router 监听，别人连这个）
backend = "127.0.0.1:7410"    # 后端（你的服务真实监听，要和上面 systemd 对上）
up      = "systemctl start lyco-mytool"
down    = "systemctl stop lyco-mytool"
idle_timeout_secs = 300
```

**3. 校验**

```bash
lyco-router check --root /etc/lyco-router
systemctl restart lyco-router      # 目前没有热加载，加完要重启一次
```

> 💡 其实不用手写：`lyco-router add --kind custom --title 我的工具` 会**自动分配端口并生成这份文件**，
> 你只要把 systemd 单元的监听端口改成它分配的那个。

---

## 模板 B：远程 API（最简单）

```bash
lyco-router add --kind llm --url https://api.example.com/v1 --key sk-xxx
```

生成的：

```toml
[service]
name = "example"
kind = "llm"
mode = "remote"
[remote]
base_url    = "https://api.example.com/v1"
api_key_env = "LYCO_EXAMPLE_KEY"     # 密钥只存环境变量名，不落盘
```

然后 `export LYCO_EXAMPLE_KEY=sk-xxx` 即可。**不占端口、不需要起停。**

---

## 模板 C：CLI 工具（包一层 HTTP）

你的工具是 CLI（跑完就退），想让它出现在管理页、能按需拉起，就包一层薄 HTTP：

```python
# /opt/lyco/mytool-server.py
import subprocess
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

class H(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/health":
            self.send_response(200); self.end_headers(); self.wfile.write(b"ok")
        else:
            self.send_response(404); self.end_headers()

    def do_POST(self):
        n = int(self.headers.get("Content-Length", 0))
        arg = self.rfile.read(n).decode()
        # 把你的 CLI 包进来
        out = subprocess.run(["/usr/local/bin/mytool", arg],
                             capture_output=True, text=True, timeout=300)
        body = (out.stdout or out.stderr).encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/plain; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

ThreadingHTTPServer(("127.0.0.1", 7410), H).serve_forever()
```

然后走**模板 A** 注册（systemd 里 `ExecStart=/usr/bin/python3 /opt/lyco/mytool-server.py`）。

### 或者：当**工作流的一步**（不注册服务）

如果这个 CLI 只是流水线里的一环，别注册服务，直接写工作流：

```toml
# workflows/myflow/workflow.toml
api_version = "lyco/v1"

[workflow]
name  = "myflow"
title = "我的流水线"

[[step]]
id   = "a"
use  = "mytool"        # 引用你的服务名，或 kind
in   = "{{input.file}}"
out  = "step1.out"
```

---

## 模板 D：Agent 工具（用 lilyco 写，天生出 MCP）

如果你的工具是**给 AI Agent 调**的（DSH / Claude / Cursor），别自己搓 MCP —— 用
[lilyco](https://github.com/lilyco-42/lilyco) 写，**一个 struct 出四端**：

```rust
use lilyco::prelude::*;

/// 跑一段 bash 脚本
#[derive(App)]
#[app(name = "run", about = "在 Windows 上跑满血 bash 语法", run = "run_it")]
struct Brush {
    /// 脚本内容
    #[arg(about = "要跑的脚本")]
    script: String,
}

fn run_it(app: &Brush, ctx: &Context) -> Result<serde_json::Value, AppError> {
    let out = std::process::Command::new("nu").arg("-c").arg(&app.script).output()?;
    let r = serde_json::json!({ "stdout": String::from_utf8_lossy(&out.stdout) });
    ctx.done(r.clone(), 0);
    Ok(r)
}

fn main() {
    let mut registry = Registry::new();
    registry.register(RegisteredCommand::from_app::<Brush>()).unwrap();
    if std::env::args().any(|a| a == "--mcp") {
        lilyco::serve_mcp(registry);          // ← Agent 直接调
    } else {
        lilyco::run_cli_registry("dsh-tool-brush", registry);
    }
}
```

拿到手就有：**CLI / TUI / Web / MCP** 四端，`--mcp` 给 Agent，`--schema` 给 AI 读。

---

## 实例：把 `dsh-tool-brush` 接进来

`dsh-tool-brush` 是**给 Agent 用的 CLI**（不是服务），所以有两条路：

| 路线 | 做法 | 结果 |
|---|---|---|
| **D（推荐）** | 它本来就用 lilyco 写 → 直接 `dsh-tool-brush --mcp`，Agent 就能调 | 不改代码 |
| **C** | 想要它出现在管理页 → 包一层 HTTP → 注册成 `local` 服务 | 加 20 行 wrapper |

**如果走 C，完整步骤：**

```bash
# 1) 装 CLI 到 PATH
install -m0755 dsh-tool-brush /usr/local/bin/dsh-tool-brush

# 2) 放 wrapper（上面的模板 C）
install -m0755 mytool-server.py /opt/lyco/dsh-tool-brush-server.py

# 3) systemd 单元 lyco-brush.service（监听 127.0.0.1:7410，不 enable）
# 4) 注册（自动分配端口，再把 systemd 端口改成分配到的那个）
lyco-router add --kind custom --title "brush 脚本" --root /etc/lyco-router
# 5) 校验 + 重启
lyco-router check --root /etc/lyco-router && systemctl restart lyco-router
```

---

## 验收清单（照 SPEC §11）

- [ ] manifest 带 `api_version = "lyco/v1"`
- [ ] `name` 小写+连字符、全局唯一
- [ ] `kind` 在词表内（`asr/tts/ocr/rembg/llm/download/media/custom`）
- [ ] 端口不和别人冲突
- [ ] `up`/`down` **快速返回**（`systemctl` 风格；**别放前台长驻进程**）
- [ ] systemd 单元的监听端口 == manifest 的 `backend`
- [ ] `lyco-router check --root /etc/lyco-router` 通过
- [ ] 重启 lyco-router 后，管理页 `http://<板子IP>:8080/` 能看到它

---

## 三个已知的坑

1. **端口耦合**：`add` 会自动分配端口，但 systemd 单元的端口是你手写的 —— **必须人工对上**。
   （v0.3 会让 `add` 连单元一起生成。）
2. **没有热加载**：加完服务要 `systemctl restart lyco-router`。
3. **`up` 不能是前台进程**：lyco-router 用 `.status()` 等命令退出，放个前台 server 会卡死整条链路。
