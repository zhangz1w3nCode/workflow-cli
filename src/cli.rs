use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "workflow", about = "Ocean 工作流执行引擎")]
pub struct Cli {
    /// 项目根（含 .workflows 的目录），缺省从 cwd 向上查找
    #[arg(long, global = true)]
    pub root: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// 列出所有可用工作流源文件
    List,

    /// 创建实例或列出实例
    Instance(InstanceArgs),

    /// 拉取下一个节点内容
    Next {
        #[arg(long)]
        instance: String,
        #[arg(long)]
        json: bool,
    },

    /// 交产物并推进
    Complete {
        #[arg(long)]
        instance: String,
        #[arg(long)]
        output: Option<String>,
        #[arg(long)]
        output_file: Option<String>,
    },

    /// 标记失败
    Fail {
        #[arg(long)]
        instance: String,
        #[arg(long)]
        reason: String,
    },

    /// 决策分支选择
    Choose {
        #[arg(long)]
        instance: String,
        #[arg(long)]
        branch: String,
        #[arg(long)]
        reason: Option<String>,
    },

    /// 查看实例进度
    Status {
        #[arg(long)]
        instance: String,
        #[arg(long)]
        json: bool,
    },
    /// 产物查询与搜索
    Artifact(ArtifactArgs),
    /// 暂存或查看 Agent 上下文
    Context(ContextArgs),
}

#[derive(Args)]
pub struct InstanceArgs {
    /// workflow-name（创建实例）或 "list"（列出实例）
    #[arg(value_name = "workflow-name-or-list")]
    pub target: String,

    /// 初始任务，记录进 process.md 供追溯
    #[arg(long)]
    pub input: Option<String>,

    #[arg(long)]
    pub max_steps: Option<usize>,
    #[arg(long)]
    pub max_loop: Option<usize>,
    #[arg(long)]
    pub max_retry: Option<usize>,

    /// 自定义 instance-id（创建时）
    #[arg(long)]
    pub instance: Option<String>,

    /// instance list 的工作流筛选
    #[arg(long)]
    pub workflow: Option<String>,
}

#[derive(Args)]
pub struct ArtifactArgs {
    #[command(subcommand)]
    pub action: ArtifactAction,
}

#[derive(Subcommand)]
pub enum ArtifactAction {
    /// 列出实例所有产物元数据
    List {
        #[arg(long)]
        instance: String,
        #[arg(long)]
        json: bool,
    },
    /// 查看某个产物的完整内容
    View {
        #[arg(long)]
        instance: String,
        #[arg(long)]
        node: Option<String>,
        #[arg(long)]
        invoke: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// 关键词搜索产物
    Search {
        #[arg(long)]
        instance: String,
        #[arg(long)]
        keyword: String,
        #[arg(long)]
        json: bool,
    },
    /// 获取执行时间线及所有产物内容（恢复用）
    Timeline {
        #[arg(long)]
        instance: String,
        #[arg(long)]
        json: bool,
    },
    /// 对比重入节点的多次执行产物
    Diff {
        #[arg(long)]
        instance: String,
        #[arg(long)]
        node: String,
        #[arg(long, default_value_t = 3)]
        context: usize,
        #[arg(long)]
        full: bool,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Args)]
pub struct ContextArgs {
    #[command(subcommand)]
    pub action: ContextAction,
}

#[derive(Subcommand)]
pub enum ContextAction {
    /// 追加暂存 Agent 上下文
    Set {
        #[arg(long)]
        instance: String,
        #[arg(long)]
        topic: String,
        #[arg(long)]
        content: String,
    },
    /// 查看 Agent 上下文
    Get {
        #[arg(long)]
        instance: String,
        #[arg(long)]
        json: bool,
    },
}
