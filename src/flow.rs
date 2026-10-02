//! 工作流执行器：把「元素」按顺序拼接跑起来。
//!
//! - **元素** = PATH 上的一个命令（`ffmpeg` / `whisper` / `cat` …），或已注册的服务名。
//! - **工作流** = 有序步骤；**上一步的产物 = 下一步的输入**。
//! - 变量：`{{in}}` 输入路径 · `{{out}}` 产物路径 · `{{outbase}}` 产物去扩展名 · `{{outdir}}` 工作目录。

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use crate::config::WorkflowManifest;

pub struct StepResult {
    pub index: usize,
    pub id: String,
    pub element: String,
    pub cmdline: String,
    pub code: i32,
    pub ms: u128,
    pub out: Option<String>,
    pub stdout: String,
    pub stderr: String,
}

impl StepResult {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "step": self.index, "id": self.id, "element": self.element,
            "cmdline": self.cmdline, "code": self.code, "ms": self.ms,
            "out": self.out, "stdout": self.stdout, "stderr": self.stderr,
        })
    }
}

fn expand(s: &str, in_path: &Path, out_path: &Path, outdir: &Path) -> String {
    let outbase = out_path.with_extension("");
    s.replace("{{in}}", &in_path.to_string_lossy())
        .replace("{{outbase}}", &outbase.to_string_lossy())
        .replace("{{out}}", &out_path.to_string_lossy())
        .replace("{{outdir}}", &outdir.to_string_lossy())
}

/// 顺序执行工作流；`log` 用于逐步进度回调。返回（逐步结果, 最终产物路径）。
pub fn run(
    wf: &WorkflowManifest,
    input: &Path,
    outdir: &Path,
    log: &dyn Fn(String),
) -> Result<(Vec<StepResult>, PathBuf), String> {
    // 元素在 outdir 里执行，所以输入与产物目录都必须是**绝对路径**，否则相对路径会解析错
    let input = std::path::absolute(input).map_err(|e| format!("输入路径绝对化失败: {e}"))?;
    let outdir = std::path::absolute(outdir).map_err(|e| format!("产物目录绝对化失败: {e}"))?;
    std::fs::create_dir_all(&outdir)
        .map_err(|e| format!("建工作目录 {} 失败: {e}", outdir.display()))?;
    if !input.exists() {
        return Err(format!("输入不存在: {}", input.display()));
    }
    if wf.steps.is_empty() {
        return Err("工作流没有任何步骤".into());
    }

    let mut cur = input.to_path_buf();
    let mut results = Vec::new();

    for (i, step) in wf.steps.iter().enumerate() {
        let out_name = if step.out.trim().is_empty() {
            format!("step{}.out", i + 1)
        } else {
            step.out.trim().to_string()
        };
        let out_path = outdir.join(&out_name);
        let args: Vec<String> = step
            .args
            .iter()
            .map(|a| expand(a, &cur, &out_path, &outdir))
            .collect();

        let label = if step.id.is_empty() { step.use_.clone() } else { step.id.clone() };
        let cmdline = format!("{} {}", step.use_, args.join(" "));
        log(format!("[{}/{}] {} : {}", i + 1, wf.steps.len(), label, cmdline));

        let t0 = Instant::now();
        let mut cmd = Command::new(&step.use_);
        cmd.args(&args).current_dir(&outdir);
        if step.pipe {
            let f = std::fs::File::open(&cur)
                .map_err(|e| format!("步骤 `{label}` 打不开输入 {}: {e}", cur.display()))?;
            cmd.stdin(std::process::Stdio::from(f));
        }
        let output = cmd
            .output()
            .map_err(|e| format!("步骤 `{}` 无法执行元素 `{}`: {e}", label, step.use_))?;
        let code = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        // 产物：优先看 out 文件；若元素只往 stdout 写（cat/echo/grep），把 stdout 落成产物
        if !out_path.exists() && !stdout.is_empty() {
            let _ = std::fs::write(&out_path, stdout.as_bytes());
        }
        let produced = out_path.exists();

        results.push(StepResult {
            index: i + 1,
            id: step.id.clone(),
            element: step.use_.clone(),
            cmdline,
            code,
            ms: t0.elapsed().as_millis(),
            out: if produced { Some(out_path.display().to_string()) } else { None },
            stdout: stdout.clone(),
            stderr: stderr.clone(),
        });

        if code != 0 {
            let tail = if stderr.trim().is_empty() {
                String::new()
            } else {
                format!("；stderr: {}", stderr.trim())
            };
            return Err(format!("步骤 `{label}` 退出码 {code}{tail}"));
        }
        if !produced {
            return Err(format!("步骤 `{label}` 没有产出 `{out_name}`"));
        }
        cur = out_path;
    }

    Ok((results, cur))
}
