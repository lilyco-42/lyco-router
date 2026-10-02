# lyco 服务与工作流规范 · `lyco/v1`

> 目标：**任何人（包括不懂技术的顾客）按这份规范丢一个文件夹进来，就等于新增一个服务/工作流**，
> 不改核心代码、不重启守护进程。路由器、CLI、网页台、MCP 全部从同一份 manifest 渲染。

---

## 0. 设计原则

1. **约定优于配置，配置优于代码** —— 能靠目录约定解决的，不进配置；能靠配置解决的，不写代码。
2. **一份 manifest，四端同源** —— CLI / TUI / Web / MCP 都读它，改一处处处生效。
3. **能力可替换** —— 换 whisper 模型、换 TTS 引擎 = 改一个字段，不动流程。
4. **顾客永不碰端口/进程** —— 那是 L1 的事，规范负责推导。

---

## 1. 概念分层

| 层 | 概念 | 一句话 |
|---|---|---|
| L1 | **Service** | 一个能起停的进程/端口（如 whisper 服务） |
| L2 | **Capability** | 一个可替换的能力（asr / tts / ocr / rembg / llm …），由某个 service 提供 |
| L3 | **Workflow** | 一串步骤，每步引用一个 capability |
| — | **Template** | 工作流的预置配方（一键添加） |
| — | **Model** | 可切换的模型 / 音色 |

---

## 2. 目录约定（drop-in）

```
<config_root>/                        # 默认 /etc/lyco-router
├── services/
│   └── <name>/
│       ├── service.toml              # 必需：服务清单
│       ├── README.md                 # 可选：给顾客看的说明
│       └── assets/                   # 可选：模型/资源
├── capabilities/
│   └── <kind>.toml                   # 可选：某能力的默认 provider
├── workflows/
│   └── <name>/
│       └── workflow.toml             # 必需：工作流定义
├── templates/
│   └── <name>.toml                   # 模板
└── models/
    └── catalog.toml                  # 模型目录
```

**铁律：文件夹名 = `name` = 全局唯一 ID。加文件即新增，删文件即移除。**

---

## 3. `service.toml`

```toml
api_version = "lyco/v1"

[service]
name  = "whisper"          # 唯一 ID：小写 + 连字符（可自动从 URL/标题派生）
kind  = "asr"              # 见 §4 词表
mode  = "local"            # local（本地进程，要起停）| remote（远程 API，无需起停）
title = "语音识别"          # 显示名（网页台卡片标题）
about = "音频 → 文字"

[runtime]
listen            = "0.0.0.0:7200"   # 前端端口（router 监听，顾客连这个）
backend           = "127.0.0.1:7201" # 后端端口（服务真实监听）
up                = "systemctl start lyco-whisper"
down              = "systemctl stop lyco-whisper"
idle_timeout_secs = 300              # 空闲多久停掉
ready_timeout_secs = 60              # 唤醒后等就绪上限

[params]                    # 可调参数 —— 网页台渲染成控件
model    = "small"          # enum：选项来自 models/catalog.toml
language = "auto"           # string
```

### 3.1 两种服务：`local` / `remote`

| mode | 是什么 | 要不要端口 | 要不要 up/down |
|---|---|---|---|
| `local` | 本地进程（whisper / rembg / galgame） | ✅ 要 | ✅ 要（systemd） |
| `remote` | 某个 AI 网站的 API（"我就有个 API"） | ❌ 不占 | ❌ 不需要 |

```toml
# remote：注册一个远程 AI API —— 只需 endpoint + 密钥环境变量名
api_version = "lyco/v1"

[service]
name  = "myai"
kind  = "llm"
mode  = "remote"
title = "某AI"

[remote]
base_url    = "https://api.example.com/v1"
api_key_env = "LYCO_MYAI_KEY"     # 只存环境变量名，密钥本体不落盘
model       = "gpt-4o-mini"
```

> `remote` 服务不监听端口、不需要起停，路由器只把它**登记为可用能力**，供工作流 `use`。

### 3.2 端口自动分配（顾客永不用手填）

- **池**：`listen` 7300–7399，`backend` 7400–7499。
- **规则**：扫全部 manifest 的已用端口 → 取**最小空闲**（= 自增的效果，但删服务能回收，比盲目自增稳）。
- 分配后**写回 manifest**（端口稳定，重启不变）。
- `mode = "remote"` **不占端口**。
- `check --all` 兜底检测冲突；避开系统已占端口（80 / 22 / 7890 / 7892 / 1053 / 9091 …）。

### 字段约束

| 字段 | 必填 | 约束 |
|---|---|---|
| `name` | ✅ | `^[a-z][a-z0-9-]*$`，全局唯一 |
| `kind` | ✅ | 必须在 §4 词表内 |
| `title` / `about` | ✅ | 给顾客看，中文可 |
| `listen` / `backend` | ✅ | `host:port`；**不同服务端口不得冲突** |
| `up` / `down` | ✅ | 必须**快速返回**（`systemctl` 风格）；禁止前台长驻进程 |
| `idle_timeout_secs` | 否 | 默认 300 |
| `ready_timeout_secs` | 否 | 默认 30 |
| `[params]` | 否 | 键名任意；值类型 → 控件（bool/enum/number/string） |

---

## 4. `kind` 词表 + 接口契约

| kind | 输入 | 输出 | 说明 |
|---|---|---|---|
| `asr` | audio | text | 语音 → 文字 |
| `tts` | text | audio | 文字 → 语音 |
| `ocr` | image | text | 图 → 文字 |
| `rembg` | image | image (png/alpha) | 抠图 |
| `llm` | text | text | 大模型对话/补全 |
| `download` | url | file | 下载资源 |
| `media` | file | file | ffmpeg 类转码/抽音 |
| `custom` | 任意 | 任意 | 兜底；参数在 `[params]` 自述 |

> 新增 kind 需在本文档登记。**同一 kind 可以有多个 service**（whisper.cpp / faster-whisper），
> 工作流通过 `capabilities/<kind>.toml` 选默认，或直接 `use = "<service-name>"` 指定。

---

## 5. `workflow.toml`

```toml
api_version = "lyco/v1"

[workflow]
name  = "video2subtitle"
title = "视频转字幕"
about = "视频 → 抽音 → 转写 → SRT"

[[step]]
id   = "audio"
use  = "ffmpeg"                                     # ← 元素（PATH 上的命令）
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
```

> **元素 = 一个命令**（`ffmpeg` / `whisper` / `cat` …，或已注册的服务名）。
> **工作流 = 元素按顺序拼接；上一步的产物 = 下一步的输入。**

### 变量

| 变量 | 含义 |
|---|---|
| `{{in}}` | **上一步的产物**（第一步则是工作流入参） |
| `{{out}}` | 本步产物路径（`<outdir>/<step.out>`） |
| `{{outbase}}` | 本步产物**去掉扩展名**（给 `-of` 这类参数用） |
| `{{outdir}}` | 本次运行的工作目录 |

### 输入怎么交给元素

| 方式 | 写法 | 适合 |
|---|---|---|
| **路径参数** | `args = ["-i", "{{in}}", …, "{{out}}"]` | ffmpeg / whisper 这类「吃路径、写路径」的 |
| **stdin 管道** | `pipe = true` | cat / tr / sed / grep 这类从 stdin 读的 |
| **stdout 落盘** | 自动：若 `out` 文件没生成但 stdout 非空 → 写入 | `cat` / `echo` 这类只往 stdout 写的 |

### 规则

- 步骤**按声明顺序**执行；**失败即停**，报「哪一步 + 退出码 + stderr」。
- `use` 可以是 **PATH 上的命令** / **capability kind** / **已注册的服务名**。
- 每步必须在 `out` 上生成产物，否则报错。
- `check` 验证 `use` 可用性（kind / 服务名 / PATH 三选一）。

### 执行

```bash
lyco-router run --name video2subtitle --input 视频.mp4
# 或 API
curl -X POST http://<板子IP>:8080/api/workflows/video2subtitle/run \
  -H 'Content-Type: application/json' -d '{"input":"/path/视频.mp4"}'
```

返回逐步结果（`cmdline` / `code` / `ms` / `out` / `stdout` / `stderr`）+ 最终产物路径。

---

## 6. `templates/<name>.toml`

```toml
api_version = "lyco/v1"

[template]
name  = "video2subtitle"
title = "视频转字幕"
about = "一键添加「视频转字幕」工作流"

[requires]
capabilities = ["media", "asr", "subtitle"]   # 缺哪个就提示装哪个

[defaults]
"asr.model"       = "small"
"subtitle.format" = "srt"
```

---

## 7. `models/catalog.toml`

```toml
[[model]]
id       = "whisper-small"
kind     = "asr"
provider = "whisper.cpp"
title    = "Whisper Small"
size     = "466MB"
url      = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin"
sha256   = "<可选，校验用>"
```

> 顾客在网页台只看到下拉框；选了 `id` → 自动下载到 `assets/` → 改 `service.toml` 的 `[params].model` → 只重启该服务。

---

## 8. 发现 / 热加载 / 校验

- **发现**：启动时扫 §2 四个目录；`watch` 文件变更 → **热加载**（守护进程不重启）。
- **校验**：`lyco-router check --all` 检查 ——
  1. 必填字段齐全、`name` 合法唯一；
  2. `kind` 在词表内；
  3. **端口不冲突**（跨所有 service 的 listen/backend）；
  4. 工作流引用的 kind/service/template 存在；
  5. 循环引用（工作流 A 调 B、B 调 A）。
- 单条坏 manifest **只跳过该条**，不拖垮整体。

---

## 9. 版本与兼容

- 每个 manifest **必须**带 `api_version`，当前 `lyco/v1`。
- 加字段 = 可选（向后兼容）；删/改语义 = 升 `lyco/v2`，路由器同时认两个版本一段时间。

---

## 10. 完整示例：30 秒新增一个服务

**需求**：加一个「edge-tts 配音」服务。

```
services/edge-tts/service.toml
```

```toml
api_version = "lyco/v1"

[service]
name  = "edge-tts"
kind  = "tts"
title = "Edge 配音"
about = "文字 → 语音（微软 Edge TTS）"

[runtime]
listen  = "0.0.0.0:7300"
backend = "127.0.0.1:7301"
up      = "systemctl start lyco-edge-tts"
down    = "systemctl stop lyco-edge-tts"
idle_timeout_secs = 180

[params]
voice = "zh-CN-XiaoxiaoNeural"
rate  = "+0%"
```

→ 存盘即生效。网页台多出一张「Edge 配音」卡片，工作流里就能 `use = "tts"` 或 `use = "edge-tts"`。

---

## 11. 新增服务 checklist

- [ ] `services/<name>/service.toml` 字段齐全，带 `api_version`
- [ ] `name` 合法且唯一，`kind` 在词表内
- [ ] 端口不与现有服务冲突
- [ ] `up` / `down` 快速返回（`systemctl` 风格）
- [ ] `lyco-router check --all` 通过
- [ ] 若要被工作流引用：加 `capabilities/<kind>.toml`，或工作流直接 `use = "<name>"`
- [ ] （可选）配 `templates/<name>.toml` 让顾客一键添加

---

## 12. 极简注册（scaffold）—— 顾客只填 2~3 项

Rails 的 `rails generate` 思路：**一条命令生成整份 manifest，名称/端口/目录/默认值全自动**。

```bash
# ① 注册一个远程 AI API（最常见的"我就有个 API"）
lyco-router add --kind llm --url https://api.deepseek.com/v1 --key sk-xxx
#   自动：name=deepseek（从域名派生）、mode=remote、不占端口 → services/deepseek/service.toml

# ② 注册一个本地引擎
lyco-router add --kind rembg --title "抠图"
#   自动：name=rembg、listen=7300、backend=7400、up/down 骨架 → services/rembg/service.toml
```

**约定优先的推导规则**：

| 顾客给 | 自动推导 |
|---|---|
| `--url` | `name` 从域名派生；`mode=remote`；`[remote].base_url` |
| `--type`（kind） | 目录位置、卡片分类、默认 params、`up/down` 骨架 |
| `--key` | 写进 `api_key_env` 提示（密钥本体落环境变量，不写 manifest） |
| 端口 | 扫最小空闲（§3.2） |
| 其余 | 全默认值，**先能跑再微调** |

- 只 `--type` 必填；能给默认的一律给默认。
- 终端里直接跑 `lyco-router add` → lilyco 自动渲染 **TUI 表单**（下拉选类型），小白零学习成本。
- 生成后可 `lyco-router check --all` 校验。

### 12.1 LLM 辅助注册（可选）

```bash
lyco-router add --url https://api.example.com/v1 --key sk-xxx --ai
# 或  lyco-router add --describe "阿里云语音合成 API，key=xxx"
```

- 调 `lyco_agent` / `lyco_chat`（或任意 OpenAI 兼容后端）读文档/描述 → **猜 kind、参数、默认 model**。
- **原则：AI 只做填空题。** 产物必须是确定、可读、人可改的 manifest；AI 猜错也只是改一个字段，不影响可复现性。
- AI 不可用时自动退化为纯默认值（不能因为 AI 挂了就注册不了）。

---

## 13. 一句话总结

**L1 管进程、L2 管能力、L3 管流程；规范管「怎么加」，界面管「谁来用」。**
顾客只做两件事：**选模板、填下拉**。其余全是约定。
