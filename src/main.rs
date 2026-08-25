mod artifact;
mod artifact_query;
mod cli;
mod executor;
mod graph;
mod instance;
mod limits;
mod model;
mod state;

use clap::Parser;
use cli::{Cli, Command};
use state::Limits;

fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let cli = Cli::parse();
    let root = resolve_root(cli.root.as_deref())?;

    match cli.command {
        Command::List => {
            println!("{}", executor::list_workflows(&root)?);
        }
        Command::Instance(args) => {
            if args.target == "list" {
                println!("{}", executor::list_instances(&root, args.workflow.as_deref())?);
            } else {
                let id = args.instance.unwrap_or_else(gen_id);
                let limits = Limits {
                    max_steps: args.max_steps.unwrap_or(100),
                    max_loop: args.max_loop.unwrap_or(10),
                    max_retry: args.max_retry.unwrap_or(2),
                };
                instance::create(&root, &args.target, &id, args.input.as_deref(), limits)?;
                println!("{id}");
            }
        }
        Command::Next { instance: id, json } => {
            let wf = instance_workflow(&root, &id)?;
            print!("{}", executor::next(&root, &wf, &id, json)?);
            println!();
        }
        Command::Complete { instance: id, output, output_file } => {
            let wf = instance_workflow(&root, &id)?;
            let content = read_output(output, output_file)?;
            println!("{}", executor::complete(&root, &wf, &id, &content)?);
        }
        Command::Fail { instance: id, reason } => {
            let wf = instance_workflow(&root, &id)?;
            println!("{}", executor::fail(&root, &wf, &id, &reason)?);
        }
        Command::Choose { instance: id, branch, reason } => {
            let wf = instance_workflow(&root, &id)?;
            println!("{}", executor::choose(&root, &wf, &id, &branch, reason.as_deref())?);
        }
        Command::Status { instance: id, json } => {
            let wf = instance_workflow(&root, &id)?;
            log_trace_command(&root, &wf, &id, "status");
            println!("{}", executor::status(&root, &wf, &id, json)?);
        }
        Command::Artifact(args) => {
            match args.action {
                cli::ArtifactAction::List { instance: id, json } => {
                    let wf = instance_workflow(&root, &id)?;
                    log_trace_command(&root, &wf, &id, "artifact list");
                    println!("{}", artifact_query::list(&root, &wf, &id, json)?);
                }
                cli::ArtifactAction::View { instance: id, node, invoke, json } => {
                    let wf = instance_workflow(&root, &id)?;
                    log_trace_command(&root, &wf, &id, "artifact view");
                    println!("{}", artifact_query::view(&root, &wf, &id, node.as_deref(), invoke.as_deref(), json)?);
                }
                cli::ArtifactAction::Search { instance: id, keyword, json } => {
                    let wf = instance_workflow(&root, &id)?;
                    log_trace_command(&root, &wf, &id, "artifact search");
                    println!("{}", artifact_query::search(&root, &wf, &id, &keyword, json)?);
                }
                cli::ArtifactAction::Timeline { instance: id, json } => {
                    let wf = instance_workflow(&root, &id)?;
                    log_trace_command(&root, &wf, &id, "artifact timeline");
                    println!("{}", artifact_query::timeline(&root, &wf, &id, json)?);
                }
                cli::ArtifactAction::Diff { instance: id, node, context, full, json } => {
                    let wf = instance_workflow(&root, &id)?;
                    log_trace_command(&root, &wf, &id, "artifact diff");
                    println!("{}", artifact_query::diff(&root, &wf, &id, &node, json, context, full)?);
                }
            }
        }
        Command::Context(args) => {
            match args.action {
                cli::ContextAction::Set { instance: id, topic, content } => {
                    let wf = instance_workflow(&root, &id)?;
                    log_trace_command(&root, &wf, &id, "context set");
                    println!("{}", artifact_query::context_set(&root, &wf, &id, &topic, &content)?);
                }
                cli::ContextAction::Get { instance: id, json } => {
                    let wf = instance_workflow(&root, &id)?;
                    log_trace_command(&root, &wf, &id, "context get");
                    println!("{}", artifact_query::context_get(&root, &wf, &id, json)?);
                }
            }
        }
    }
    Ok(())
}

fn resolve_root(explicit: Option<&str>) -> Result<std::path::PathBuf, String> {
    if let Some(p) = explicit {
        return Ok(std::path::PathBuf::from(p));
    }
    let mut cur = std::env::current_dir().map_err(|e| format!("获取 cwd 失败: {e}"))?;
    loop {
        if cur.join(".workflows").is_dir() {
            return Ok(cur);
        }
        if !cur.pop() {
            break;
        }
    }
    Err("未找到 .workflows 目录，请用 --root 指定项目根".into())
}

fn gen_id() -> String {
    let now = chrono::Local::now();
    let ts = now.format("%Y%m%dT%H%M%S").to_string();
    let suffix = now.timestamp_subsec_millis() % 10000;
    format!("{ts}-{suffix:04x}")
}

fn instance_workflow(root: &std::path::Path, instance_id: &str) -> Result<String, String> {
    use walkdir::WalkDir;
    for entry in WalkDir::new(root.join(".workflows")).max_depth(3).into_iter().filter_map(|e| e.ok()) {
        let p = entry.path();
        if p.is_dir()
            && p.file_name().and_then(|n| n.to_str()) == Some(instance_id)
            && p.parent().and_then(|x| x.file_name().and_then(|n| n.to_str())) == Some("instance") {
            return p.parent().and_then(|x| x.parent()).and_then(|x| x.file_name())
                .and_then(|n| n.to_str()).map(|s| s.to_string())
                .ok_or_else(|| "无法确定工作流名".into());
        }
    }
    Err(format!("实例不存在: {instance_id}"))
}

fn read_output(output: Option<String>, output_file: Option<String>) -> Result<String, String> {
    if let Some(f) = output_file {
        return std::fs::read_to_string(&f).map_err(|e| format!("读取产物文件失败 {f}: {e}"));
    }
    if let Some(o) = output {
        return Ok(o);
    }
    use std::io::Read;
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf).map_err(|e| format!("读取 stdin 失败: {e}"))?;
    Ok(buf)
}

fn log_trace_command(root: &std::path::Path, workflow: &str, instance_id: &str, command: &str) {
    let inst_dir = root.join(".workflows").join(workflow).join("instance").join(instance_id);
    let _ = state::log_trace(&inst_dir, state::TraceLogEntry {
        ts: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        command: command.into(),
        node: None,
        invoke: None,
        status: None,
        branch: None,
    });
}
