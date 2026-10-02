# lyco-router

**按需服务路由器** —— 像 [`itzg/mc-router`](https://github.com/itzg/mc-router) 那样：
常驻的只是一层薄薄的路由器（几十 MB），真正的服务（rembg / whisper / galgame …）
**平时完全不在跑**；有人连前端端口才唤醒后端，空闲超时自动停掉。

用 [lilyco](https://github.com/lilyco-42/lilyco) 框架写成：一个 struct 出 **CLI / TUI / Web / MCP** 四端。

配置规范见 **[SPEC.md](SPEC.md)**（`lyco/v1`）：约定优于配置，加一个文件夹 = 加一个服务。

## 为什么

单板机（如 Radxa Cubie A7A）内存只有几 GB，不可能让 rembg、whisper、galgame
同时常驻。lyco-router 让它们变成「**按需拉起**」：

```
客户端 ──► lyco-router:7300 ──(没人连时不占资源)
                 │  有人连
                 ▼
           up = "systemctl start lyco-rembg"   ← 才拉起后端
                 │  就绪
                 ▼
           代理到 127.0.0.1:7400 ──► rembg
                 │  空闲 300s
                 ▼
           down = "systemctl stop lyco-rembg"  ← 自动停掉
```

## 构建

```bash
cargo build --release          # 产物 target/release/lyco-router
```

## 快速开始

```bash
# 1. 建配置目录骨架（含示例）
lyco-router init --root /etc/lyco-router

# 2. 注册服务 —— 顾客只填 2~3 项，其余全自动
lyco-router add --kind llm --url https://api.deepseek.com/v1 --key sk-xxx
#   → name 从域名派生（deepseek）、mode=remote、不占端口

lyco-router add --kind rembg --title "抠图"
#   → name=rembg、listen=7300、backend=7400、up/down 骨架

# 3. 看 / 校验
lyco-router list  --root /etc/lyco-router
lyco-router check --root /etc/lyco-router

# 4. 跑守护进程（前台）
lyco-router serve --root /etc/lyco-router
```

TUI / Web 界面（桌面用，需 `--features tui,web` 构建）：

```bash
lyco-router add --gui        # Web 表单
lyco-router --mcp            # MCP 服务器（Agent 可直接注册服务）
```

## 配置目录（drop-in）

```
/etc/lyco-router/
├── services/<name>/service.toml     # 服务
├── workflows/<name>/workflow.toml   # 工作流
├── templates/<name>.toml            # 模板
└── models/catalog.toml              # 模型目录
```

**文件夹名 = name = 全局唯一 ID。加文件即新增，删文件即移除。**

### 两种服务：`local` / `remote`

| mode | 是什么 | 端口 | up/down |
|---|---|---|---|
| `local` | 本地进程（whisper / rembg / galgame） | ✅ 自动分配 | ✅ systemd |
| `remote` | 某个 AI 网站的 API（"我就有个 API"） | ❌ 不占 | ❌ 不需要 |

```toml
# remote —— 只需 endpoint + 密钥环境变量名（密钥本体不落盘）
[service]
name = "deepseek"
kind = "llm"
mode = "remote"
[remote]
base_url    = "https://api.deepseek.com/v1"
api_key_env = "LYCO_DEEPSEEK_KEY"
```

```toml
# local —— 端口由 `add` 自动分配
[service]
name = "rembg"
kind = "rembg"
mode = "local"
[runtime]
listen  = "0.0.0.0:7300"
backend = "127.0.0.1:7400"
up      = "systemctl start lyco-rembg"
down    = "systemctl stop lyco-rembg"
```

**端口自动分配**：池 `listen 7300–7399` / `backend 7400–7499`，扫最小空闲（删服务能回收），分配后写回 manifest。

## 在板子上常驻（systemd）

```ini
# /etc/systemd/system/lyco-router.service
[Unit]
Description=lyco-router 按需服务路由器
After=network-online.target
Wants=network-online.target

[Service]
ExecStart=/usr/local/bin/lyco-router serve --root /etc/lyco-router
Restart=on-failure
RestartSec=3

[Install]
WantedBy=multi-user.target
```

```bash
sudo install -m0755 target/release/lyco-router /usr/local/bin/lyco-router
sudo lyco-router init --root /etc/lyco-router
sudo systemctl enable --now lyco-router
```

## 与 mc-router 的关系

| | mc-router | lyco-router |
|---|---|---|
| 面向 | Minecraft（读握手包路由） | 任意 TCP 服务（按端口） |
| 唤醒 | webhook（`auto-scale-webhook-url`） | 本地 shell 命令（`up`/`down`） |
| 空闲停 | ✅ | ✅ |
| 注册 | 手写配置 | `add` scaffold（端口/名字自动） |
| 四端 | 仅 CLI | CLI / TUI / Web / MCP（lilyco） |

## 许可

MIT
