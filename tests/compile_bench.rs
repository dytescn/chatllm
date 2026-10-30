use anyhow::{Context, Result};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

/// 项目根目录
fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// 统计目录下文件数量与总大小
fn dir_stats(dir: &Path) -> (usize, u64) {
    let mut count = 0usize;
    let mut bytes = 0u64;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                let (c, b) = dir_stats(&p);
                count += c;
                bytes += b;
            } else if p.is_file() {
                count += 1;
                bytes += entry.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    (count, bytes)
}

/// 读取 CPU 核心数（逻辑核）
fn cpu_cores() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

/// 读取 /proc/cpuinfo 中的型号（Linux）
fn cpu_model() -> String {
    fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("model name"))
                .and_then(|l| l.split(':').nth(1))
                .map(|s| s.trim().to_string())
        })
        .unwrap_or_else(|| "unknown".into())
}

/// 读取系统内存总量（KB，来自 /proc/meminfo）
fn mem_total_kb() -> u64 {
    fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("MemTotal"))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|v| v.parse().ok())
        })
        .unwrap_or(0)
}

/// 执行命令并计时，返回耗时与退出状态
fn timed_cmd(cmd: &str, args: &[&str], cwd: &Path) -> Result<(Duration, bool, String)> {
    let start = Instant::now();
    let output = Command::new(cmd)
        .args(args)
        .current_dir(cwd)
        .output()
        .with_context(|| format!("执行 {} {:?} 失败", cmd, args))?;
    let elapsed = start.elapsed();
    let ok = output.status.success();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    Ok((elapsed, ok, stderr))
}

/// 解析 cargo 输出的编译耗时（从 stderr 中提取 "Finished" 行的时间）
fn parse_cargo_time(stderr: &str) -> Option<Duration> {
    for line in stderr.lines() {
        if line.contains("Finished") && line.contains("in") {
            // 形如：Finished dev [unoptimized + debuginfo] target(s) in 12.34s
            if let Some(idx) = line.rfind("in ") {
                let t = line[idx + 3..].trim();
                return parse_duration_str(t);
            }
        }
    }
    None
}

/// 解析 "12.34s" / "1m 23s" 这类字符串
fn parse_duration_str(s: &str) -> Option<Duration> {
    let s = s.trim();
    if let Some(rest) = s.strip_suffix('s') {
        if let Some((m, sec)) = rest.split_once('m') {
            let mins: f64 = m.trim().parse().ok()?;
            let secs: f64 = sec.trim().parse().ok()?;
            Some(Duration::from_secs_f64(mins * 60.0 + secs))
        } else {
            Some(Duration::from_secs_f64(rest.trim().parse().ok()?))
        }
    } else {
        None
    }
}

/// 统计 target 目录大小（编译产物）
fn target_size(root: &Path) -> u64 {
    let target = root.join("target");
    if target.exists() {
        dir_stats(&target).1
    } else {
        0
    }
}

/// 统计源文件数量与行数（src + tests + benches）
fn source_stats(root: &Path) -> (usize, usize) {
    let mut files = 0usize;
    let mut lines = 0usize;
    for sub in ["src", "tests", "benches", "examples"] {
        let dir = root.join(sub);
        if dir.exists() {
            collect_sources(&dir, &mut files, &mut lines);
        }
    }
    (files, lines)
}

fn collect_sources(dir: &Path, files: &mut usize, lines: &mut usize) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                collect_sources(&p, files, lines);
            } else if p.extension().map(|e| e == "rs").unwrap_or(false) {
                *files += 1;
                if let Ok(content) = fs::read_to_string(&p) {
                    *lines += content.lines().count();
                }
            }
        }
    }
}

/// 读取 scifact 数据集大小（用于上下文关联）
fn dataset_stats(root: &Path) -> serde_json::Value {
    let base = root.join("datasets/scifact");
    if !base.exists() {
        return json!({"exists": false});
    }
    let corpus = base.join("corpus.jsonl");
    let queries = base.join("queries.jsonl");
    json!({
        "exists": true,
        "corpus_bytes": fs::metadata(&corpus).map(|m| m.len()).unwrap_or(0),
        "queries_bytes": fs::metadata(&queries).map(|m| m.len()).unwrap_or(0),
    })
}

#[test]
fn compile_bench() -> Result<()> {
    let root = project_root();
    let cpu = cpu_cores();
    let model = cpu_model();
    let mem_kb = mem_total_kb();
    let (src_files, src_lines) = source_stats(&root);
    let ds = dataset_stats(&root);

    println!("========== 环境信息 ==========");
    println!("CPU 型号      : {}", model);
    println!("逻辑核心数    : {}", cpu);
    println!("内存总量      : {:.2} GiB", mem_kb as f64 / 1024.0 / 1024.0);
    println!("项目根目录    : {}", root.display());
    println!("源文件数      : {}", src_files);
    println!("源码总行数    : {}", src_lines);
    println!("数据集        : {}", ds);

    // ===== 阶段 0：清理 =====
    println!("\n========== 阶段 0：cargo clean ==========");
    let (t, ok, _) = timed_cmd("cargo", &["clean"], &root)?;
    println!("cargo clean 耗时: {:?} (成功={})", t, ok);
    println!("清理后 target 大小: {} bytes", target_size(&root));

    // ===== 阶段 1：全量 debug 编译（lib + bins + tests 元数据） =====
    println!("\n========== 阶段 1：全量编译（debug, 含测试） ==========");
    let start = Instant::now();
    let (_t, ok, stderr) = timed_cmd("cargo", &["build", "--tests", "--all-targets"], &root)?;
    let wall = start.elapsed();
    println!(
        "cargo build --tests --all-targets 墙钟耗时: {:?} (成功={})",
        wall, ok
    );
    if let Some(parsed) = parse_cargo_time(&stderr) {
        println!("cargo 自报耗时: {:?}", parsed);
    }
    let target_after_debug = target_size(&root);
    println!(
        "target 大小: {} bytes ({:.2} MiB)",
        target_after_debug,
        target_after_debug as f64 / 1024.0 / 1024.0
    );

    // ===== 阶段 2：增量编译（无改动，测空跑开销） =====
    println!("\n========== 阶段 2：增量空跑（无改动） ==========");
    let (t, ok, stderr) = timed_cmd("cargo", &["build", "--tests", "--all-targets"], &root)?;
    println!("无改动增量耗时: {:?} (成功={})", t, ok);
    if let Some(parsed) = parse_cargo_time(&stderr) {
        println!("cargo 自报耗时: {:?}", parsed);
    }

    // ===== 阶段 3：增量编译（touch 一个源文件） =====
    println!("\n========== 阶段 3：增量编译（touch src/lib.rs） ==========");
    let lib_rs = root.join("src/lib.rs");
    if lib_rs.exists() {
        // 读取并写回，更新时间戳但不改内容
        let content = fs::read(&lib_rs)?;
        fs::write(&lib_rs, &content)?;
        let (t, ok, stderr) = timed_cmd("cargo", &["build", "--tests", "--all-targets"], &root)?;
        println!("touch 后增量耗时: {:?} (成功={})", t, ok);
        if let Some(parsed) = parse_cargo_time(&stderr) {
            println!("cargo 自报耗时: {:?}", parsed);
        }
    } else {
        println!("src/lib.rs 不存在，跳过");
    }

    // ===== 阶段 4：release 编译（仅 lib + bin，不含测试） =====
    println!("\n========== 阶段 4：release 编译 ==========");
    let start = Instant::now();
    let (_t, ok, stderr) = timed_cmd("cargo", &["build", "--release"], &root)?;
    let wall = start.elapsed();
    println!("cargo build --release 墙钟耗时: {:?} (成功={})", wall, ok);
    if let Some(parsed) = parse_cargo_time(&stderr) {
        println!("cargo 自报耗时: {:?}", parsed);
    }
    let target_after_release = target_size(&root);
    println!(
        "release 后 target 总大小: {} bytes ({:.2} MiB)",
        target_after_release,
        target_after_release as f64 / 1024.0 / 1024.0
    );

    // ===== 阶段 5：测试可执行文件编译（cargo test --no-run） =====
    println!("\n========== 阶段 5：cargo test --no-run ==========");
    let (t, ok, stderr) = timed_cmd("cargo", &["test", "--no-run"], &root)?;
    println!("cargo test --no-run 耗时: {:?} (成功={})", t, ok);
    if let Some(parsed) = parse_cargo_time(&stderr) {
        println!("cargo 自报耗时: {:?}", parsed);
    }

    // ===== 阶段 6：cargo check（仅类型检查，不生成代码） =====
    println!("\n========== 阶段 6：cargo check ==========");
    let (t, ok, stderr) = timed_cmd("cargo", &["check", "--tests", "--all-targets"], &root)?;
    println!("cargo check 耗时: {:?} (成功={})", t, ok);
    if let Some(parsed) = parse_cargo_time(&stderr) {
        println!("cargo 自报耗时: {:?}", parsed);
    }

    // ===== 阶段 7：统计 target 目录下各产物大小 =====
    println!("\n========== 阶段 7：target 产物分布 ==========");
    for sub in ["debug", "release", "debug/deps", "release/deps"] {
        let p = root.join("target").join(sub);
        if p.exists() {
            let (c, b) = dir_stats(&p);
            println!(
                "target/{}: {} 文件, {:.2} MiB",
                sub,
                c,
                b as f64 / 1024.0 / 1024.0
            );
        }
    }

    // ===== 汇总 =====
    println!("\n========== 汇总 ==========");
    println!("逻辑核心数        : {}", cpu);
    println!("源码文件/行数     : {} / {}", src_files, src_lines);
    println!(
        "debug target 大小 : {:.2} MiB",
        target_after_debug as f64 / 1024.0 / 1024.0
    );
    println!(
        "release target 大小: {:.2} MiB",
        target_after_release as f64 / 1024.0 / 1024.0
    );
    println!("测试完成 ✅");

    Ok(())
}
