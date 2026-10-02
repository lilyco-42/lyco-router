# 元素目录与工作流规划

> **元素 = 一个命令**（PATH 上的 `ffmpeg` / `whisper` / `cat`…，或已注册的服务名）。
> **工作流 = 元素按顺序拼接；上一步的产物 = 下一步的输入。**
>
> 这份文档记录「板子上有哪些元素可用」+「它们能拼出什么工作流」。

---

## 一、元素现状（2026-10-02 实测）

### ✅ 已就绪

| 元素 | 版本 | 用途 | 解锁 |
|---|---|---|---|
| `ffmpeg` / `ffprobe` | 7.1.5 | 转码 / 抽音 / 合成 | 全部媒体流 |
| `whisper` | whisper.cpp 1.9.4 | 语音识别（ASR） | 字幕 / 会议纪要 |
| `espeak-ng` | 1.52.0 | 文字转语音（离线 TTS） | 配音 |
| `pandoc` | 3.1.11 | 文档格式转换 | 文档流 |
| `jq` | 1.7 | JSON 处理 | 数据流 |
| `sed` `awk` `grep` `tr` `cat` | GNU | 文本处理 | 清洗 |
| `python3` | 3.13.5 | 胶水 / 脚本 | 任意 |
| `curl` `wget` | 8.14 / 1.25 | HTTP | 调服务 |
| `zstd` `xz` `7z` | — | 压缩 | 归档 |
| `brush` | 0.4.0 | bash 兼容 shell | 脚本元素 |

### ⏸ 待装（**被存储损坏阻塞**，见下）

| 元素 | apt 包 | 用途 | 解锁 |
|---|---|---|---|
| `yt-dlp` | `yt-dlp` | 在线视频下载 | 在线视频转字幕 |
| `magick` | `imagemagick` | 图片处理 | 图片批处理 |
| `tesseract` | `tesseract-ocr` + `-chi-sim` `-chi-tra` | 图片 OCR | 图片转文字 |
| `pdftotext` | `poppler-utils` | PDF 抽文字 | PDF 转文字 |
| `sox` | `sox` | 音频处理 | 降噪 / 拼接 |
| `pngquant` | `pngquant` | PNG 压缩 | 图片瘦身 |
| `qpdf` | `qpdf` | PDF 处理 | 合并 / 拆分 |
| `rg` | `ripgrep` | 文本检索 | 全文搜索 |
| `sd` | `sd` | 文本替换 | 批量改写 |
| `aria2` | `aria2` | 多线程下载 | 大文件 |

一条命令装齐（**修好存储之后**再跑）：

```bash
apt-get install -y yt-dlp imagemagick poppler-utils tesseract-ocr \
  tesseract-ocr-chi-sim tesseract-ocr-chi-tra sox pngquant qpdf ripgrep sd aria2
```

---

## 二、工作流规划

| 工作流 | 元素链 | 状态 |
|---|---|---|
| **`video2subtitle`** | `ffmpeg → whisper → cat` | ✅ **已跑通**（3.4s 视频 / 4.4s 跑完） |
| `video2mp3` | `ffmpeg` | ✅ 已建 |
| `doc2md` | `pandoc` | ✅ 已建 |
| `pdf2text` | `pdftotext` | ⏸ 待元素 |
| `image2text` | `magick → tesseract` | ⏸ 待元素 |
| `cutout` | `curl`（调 rembg 服务）`→ pngquant` | ⏸ 待元素 |
| `url2subtitle` | `yt-dlp → ffmpeg → whisper → cat` | ⏸ 待元素 |

**待规划**（有元素后即可拼）：
- 会议纪要：`ffmpeg → whisper → pandoc`
- 视频翻译配音：`ffmpeg → whisper → (llm) → espeak-ng → ffmpeg`
- 文档转 PDF：`pandoc →（需 LaTeX，板子上体积大，暂缓）`

---

## ⚠️ 三、当前阻塞：板子存储损坏

装元素时 apt 报 `E: Unable to parse package file /var/lib/apt/extended_states (1)` ——
**不是安装问题，是 SD 卡块位图损坏**（`ext4_validate_block_bitmap`，坏块组 112/240/368/497）。

**修好之前禁止**：`apt install/upgrade`、解包、大批量写入、**重启**。

修复路径见 `sd_offline_repair.ps1`（断电 → 拔卡 → 读卡器 → 离线 `e2fsck -f` → 回到板子四条验收）。
