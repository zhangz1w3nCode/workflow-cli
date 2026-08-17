# workflow-cli

Ocean 工作流执行引擎 —— 把工作流流转的**状态管理**和**路由管理**从 Agent 记忆中剥离，交给确定性命令执行。

## 背景

Ocean 画板生成的工作流（`WORKFLOW.md` + `meta-data/flow.json`）原本靠 Agent 读提示词和自身记忆驱动流转。随着工作流变长、出现环回与节点重入，Agent 会忘记下一步去哪、忘记按约定逐节点读取执行。

`workflow-cli` 用确定性状态机接管流转。Agent 只需要循环调用几个命令，不需要记忆下一步、不需要自己判断路由：

```
workflow instance <workflow-name> --input "任务"
loop:
  workflow next --instance <id>       # 拿下一步做什么
  执行 / 决策
  workflow complete --instance <id>   # 交产物并推进（或 choose / fail）
until 到达 end
```

## 特性

- **确定性路由**：以 `flow.json` 为唯一真源，CLI 计算下一步，Agent 无需记忆流转关系
- **状态机管理**：五态状态机（idle / executing / awaiting_choice / completed / aborted）
- **环回重入支持**：`invoke-id`（时间戳）隔离同一节点多次执行的产物
- **决策分支**：decision 节点由 CLI 渲染条件与分支，Agent 显式 `choose`
- **动态 mermaid 图**：`process.md` 实时维护流程图，绿=已完成、橙=当前、灰=待执行
- **结构化执行轨迹**：表格记录每次执行（状态 / 节点 / 执行ID / 执行时间），单行化去重
- **三道死循环闸**：`max_steps`（总步数）、`max_loop`（环回次数）、`max_retry`（失败重试），超限自动中止

## 构建

```bash
cargo build --release
```

产物在 `target/release/workflow`。

## 快速开始

```bash
# 1. 创建实例（拿到 instance-id）
workflow instance task-standard-pipeline --input "帮我实现登录功能"
# 输出: 20260818T003758-038b

# 2. 拉取下一个节点内容
workflow next --instance 20260818T003758-038b
# business 节点：原样输出节点任务内容
# decision 节点：输出判断条件 + 可选分支

# 3a. business 节点执行后交产物
workflow complete --instance 20260818T003758-038b --output "产物内容"

# 3b. decision 节点显式选分支
workflow choose --instance 20260818T003758-038b --branch "没问题" --reason "评审通过"

# 3c. 失败时标记，再重新 next 重试
workflow fail --instance 20260818T003758-038b --reason "执行失败原因"
workflow next --instance 20260818T003758-038b

# 4. 循环 2-3，直到 next 输出"工作流已完成"

# 5. 随时查看进度
workflow status --instance 20260818T003758-038b
```

## 命令参考

全局参数：`--root <path>` 指定项目根（含 `.workflows` 的目录），缺省从当前目录向上查找。

| 命令 | 作用 | 强制 instance-id |
|------|------|:---:|
| `workflow list` | 列出可用工作流名称 | 否 |
| `workflow instance <workflow-name>` | 创建工作流实例，返回 instance-id | 产出 id |
| `workflow instance list [--workflow <name>]` | 列出实例 | 否 |
| `workflow next` | 拉取下一个节点内容 | 是 |
| `workflow complete` | 交产物并推进 | 是 |
| `workflow fail` | 标记失败 | 是 |
| `workflow choose` | 决策分支选择 | 是 |
| `workflow status` | 查看进度 | 是 |

完整签名：

```
workflow list

workflow instance <workflow-name>
    [--input "初始任务"]                 # 记录进 process.md 供追溯
    [--max-steps N] [--max-loop N] [--max-retry N]   # 默认 100/10/2
    [--instance <id>]                   # 自定义 instance-id，缺省自动生成

workflow instance list [--workflow <name>]

workflow next --instance <id> [--json]

workflow complete --instance <id>
    [--output "产物"]                    # 短产物直接传
    [--output-file <path>]               # 长产物指向文件
    # 或 stdin：cat f | workflow complete --instance <id>

workflow fail --instance <id> --reason "失败原因"

workflow choose --instance <id> --branch "分支名" [--reason "理由"]

workflow status --instance <id> [--json]
```

## 实例目录结构

```
.workflows/{workflow-name}/instance/{instance-id}/
  instance.md                # WORKFLOW.md 副本（定义快照）
  process.md                 # 进度状态（YAML + 动态 mermaid + 执行轨迹表格）
  artifacts/{node-name}/{invoke-id}/
    detail.md                # 节点产物（CLI 写入）
    error.md                 # 失败信息（fail 时）
```

## 关键机制

- **路由真源**：`meta-data/flow.json`，CLI 据此计算下一步。
- **invoke 隔离**：每次执行生成唯一时间戳 `invoke-YYYYMMDD-HHMMSS-毫秒`，环回重入产物目录彻底隔离。
- **产物双保险**：`complete` 强制带产物（主闸），`next` 兜底校验上一个节点产物。
- **decision 显式选择**：decision 节点由 CLI 渲染条件与分支，Agent 调 `choose`；分支 `description` 非空视为 catch-all，触发环回计数。

## 开发

```bash
cargo test        # 单元测试 + 集成测试
cargo build --release
```

## 许可证

[MIT License](LICENSE)