//! IRPatch → 嘉立创后端专属 MaterializationPlan 的纯计算转换与持久化。
//!
//! preview 是 Bridge 侧纯计算：**不下发执行器、不触碰设计**。计划绑定
//! 当前 observed baseline 快照哈希；`planHash` 对计划（除自身外）的
//! canonical JSON 做 `sha256:` 摘要，同一（baseline, patch）输入必得相同
//! planHash。审批模型见 `approval.rs`。

use std::path::{Path, PathBuf};

use circuitfabric_contracts::{IrPatch, LogicalCircuitSnapshot};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;

use crate::{
    fusion::{FusionView, MappingStatus},
    observation::{observations_dir, sha256_digest, write_atomic},
};

/// 物化目标后端标识。
pub const PLAN_BACKEND: &str = "jlceda-pro";

#[derive(Debug, Error)]
pub enum PlanError {
    #[error("计划写入失败（{path}）：{source}")]
    Write { path: PathBuf, source: std::io::Error },
    #[error("计划提交失败（{from} -> {to}）：{source}")]
    Commit { from: PathBuf, to: PathBuf, source: std::io::Error },
    #[error("计划读取失败（{path}）：{source}")]
    Read { path: PathBuf, source: std::io::Error },
    #[error("计划不是有效 JSON（{path}）：{source}")]
    Parse { path: PathBuf, source: serde_json::Error },
}

/// 计划步骤种类（物化动作的保守 v1 映射）。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    SetComponentValue,
    RenameNet,
    PlaceComponent,
    DeleteComponent,
    AddNet,
    AnnotateOnly,
}

impl StepKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SetComponentValue => "set_component_value",
            Self::RenameNet => "rename_net",
            Self::PlaceComponent => "place_component",
            Self::DeleteComponent => "delete_component",
            Self::AddNet => "add_net",
            Self::AnnotateOnly => "annotate_only",
        }
    }
}

/// 回滚补偿声明（apply 执行时据此记录补偿数据）。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RollbackKind {
    RestoreProperty,
    /// 删除新建物：回滚 = 删除刚创建的图元。
    DeleteCreated,
    /// 恢复基线中被删除的物：回滚 = 用补偿数据重建。
    RestoreComponent,
    /// 无安全回滚。
    None,
}

/// 前置条件：步骤执行前必须成立的设计侧事实。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Precondition {
    pub description: String,
    pub satisfied: bool,
}

/// 步骤的回滚补偿声明。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RollbackSpec {
    pub kind: RollbackKind,
    /// 补偿数据（如 restore_component 携带基线元件 JSON）。
    pub data: Value,
}

/// 物化计划步骤。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanStep {
    pub index: u32,
    pub kind: StepKind,
    /// CircuitFabric 逻辑 ID（EDA 图元 ID 只出现在 externalPrimitiveIds）。
    pub logical_id: String,
    /// 从融合视图解析出的当前外部图元 ID。
    pub external_primitive_ids: Vec<String>,
    /// 执行器物化规格（结构类操作）：删除的位号核对、放置参数或逐脚连线锚点。
    #[serde(default)]
    pub spec: Value,
    pub preconditions: Vec<Precondition>,
    pub rollback: RollbackSpec,
}

/// 可审阅 diff 条目。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanDiffEntry {
    pub kind: DiffKind,
    pub subject: String,
    pub detail: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffKind {
    Add,
    Remove,
    Modify,
    Note,
}

/// 嘉立创后端专属物化计划（serde camelCase，直接持久化）。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterializationPlan {
    pub schema_version: u32,
    pub backend: String,
    #[serde(default)]
    pub project_id: String,
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub document: Value,
    /// 原始语义补丁进入计划哈希，不能仅对操作类型/摘要盖章。
    #[serde(default)]
    pub patch: Option<IrPatch>,
    /// 计划绑定的 observed baseline 快照哈希。
    pub baseline_snapshot_hash: String,
    /// 对计划（除本字段外）canonical JSON 的 `sha256:` 摘要。
    pub plan_hash: String,
    pub created_from_patch_rationale: String,
    pub steps: Vec<PlanStep>,
    pub affected_logical_ids: Vec<String>,
    pub risks: Vec<String>,
    /// 计划整体是否可安全回滚（任一步回滚为 none 即 false）。
    pub rollbackable: bool,
    pub diff: Vec<PlanDiffEntry>,
    /// 需要操作员在批准前明确知晓的回滚缺陷；只提示，不阻止批准。
    /// 空时不序列化，旧计划的 planHash 保持不变。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rollback_warnings: Vec<String>,
}

impl MaterializationPlan {
    /// 全部步骤的前置条件是否都满足。
    #[must_use]
    pub fn preconditions_satisfied(&self) -> bool {
        self.steps
            .iter()
            .all(|step| step.preconditions.iter().all(|precondition| precondition.satisfied))
    }
}

/// 计划文件路径 `<项目根>/observations/plan-<planHash 去前缀>.json`。
#[must_use]
pub fn plan_path(project_root: &Path, plan_hash: &str) -> PathBuf {
    let file_name = format!("plan-{}.json", plan_hash.strip_prefix("sha256:").unwrap_or(plan_hash));
    observations_dir(project_root).join(file_name)
}

/// 计算计划哈希：对 `planHash` 字段清空后的 JSON 做摘要（字段顺序由
/// struct 声明固定，数组顺序按 steps index 固定 → 确定性）。
#[must_use]
pub fn plan_digest(plan: &MaterializationPlan) -> String {
    let mut projection = plan.clone();
    projection.plan_hash.clear();
    sha256_digest(&serde_json::to_vec(&projection).expect("计划序列化不应失败"))
}

/// 观测元件记录（`observedPayload.components[]`）按位号索引。
fn observed_components(observed: &Value) -> std::collections::BTreeMap<String, Value> {
    observed
        .get("components")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let designator = item.get("designator").and_then(Value::as_str)?;
            Some((designator.to_owned(), item.clone()))
        })
        .collect()
}

fn non_empty(record: &Value, field: &str) -> bool {
    record.get(field).and_then(Value::as_str).is_some_and(|v| !v.trim().is_empty())
}

/// 原库身份（图元自报的库 UUID + 器件 UUID）；执行器无法预先核验时只能直接用于 create。
fn has_original_identity(record: &Value) -> bool {
    non_empty(record, "libraryUuid") && non_empty(record, "componentUuid")
}

/// 记录是否含库身份（原库身份或立创编号）与完整放置：可作放置来源或删除补偿数据。
fn placeable(record: &Value) -> bool {
    (has_original_identity(record) || non_empty(record, "supplierId"))
        && record.get("x").and_then(Value::as_f64).is_some()
        && record.get("y").and_then(Value::as_f64).is_some()
}

/// 由来源观测记录生成放置规格：复用库件身份，位置取源旁偏移。
fn placement_spec(source: &Value, designator: &str, name: Option<&str>) -> Value {
    json!({
        "libraryUuid": source["libraryUuid"],
        "componentUuid": source["componentUuid"],
        "supplierId": source["supplierId"],
        "x": source["x"].as_f64().unwrap_or_default() + 200.0,
        "y": source["y"].as_f64().unwrap_or_default() + 200.0,
        "rotation": source["rotation"].as_f64().unwrap_or_default(),
        "mirror": source["mirror"].as_bool().unwrap_or(false),
        "designator": designator,
        "name": name,
    })
}

/// 解析新增网络逐脚连线锚点：既有网络优先落到其导线端点（其次既有成员引脚），
/// 新网络链式相连，首个引脚画短桩。坐标锚点来自基线观测；符号锚点由执行器
/// 在写入时按位号现场解析（引脚可能属于同补丁刚放置的元件）。
fn wire_connections(
    name: &str,
    pins: &[circuitfabric_contracts::PinRef],
    observed: &Value,
    baseline: &LogicalCircuitSnapshot,
    renames: &std::collections::BTreeMap<String, String>,
) -> Value {
    let pin_spec = |pin: &circuitfabric_contracts::PinRef| {
        json!({
            "designator": pin.component_id.strip_prefix("component:").unwrap_or(&pin.component_id),
            "pin": pin.pin_id,
        })
    };
    // 目标网络的基线 ID：本补丁内刚改名的网络按旧 ID 回溯，否则按现名匹配。
    let old_net_id = renames.get(name).cloned().or_else(|| {
        baseline.nets.iter().find(|net| net.id == format!("net:{name}")).map(|net| net.id.clone())
    });
    let join_anchor: Option<Value> = old_net_id.as_ref().and_then(|net_id| {
        let old_name = net_id.strip_prefix("net:").unwrap_or_default();
        observed
            .get("wires")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|wire| wire.get("net").and_then(Value::as_str) == Some(old_name))
            .and_then(|wire| {
                let point = wire.get("points").and_then(Value::as_array)?.first()?;
                Some(json!({"x": point[0], "y": point[1]}))
            })
            .or_else(|| {
                baseline
                    .nets
                    .iter()
                    .find(|net| net.id == *net_id)
                    .and_then(|net| net.pins.first())
                    .map(|pin| json!({"pin": pin_spec(pin)}))
            })
    });
    let mut connections = Vec::with_capacity(pins.len());
    for (index, pin) in pins.iter().enumerate() {
        let anchor = join_anchor.clone().unwrap_or_else(|| {
            if index == 0 {
                json!({"stub": true})
            } else {
                json!({"pin": pin_spec(&pins[index - 1])})
            }
        });
        connections.push(json!({"pin": pin_spec(pin), "anchor": anchor}));
    }
    json!({"net": name, "connections": connections})
}

/// 把已校验的 IRPatch 转换为物化计划（纯计算，无 IO、无 EDA 副作用）。
///
/// 前置条件失败只标记在步骤上，不整体拒绝；risks 给出人类可读的风险。
#[must_use]
pub fn build_plan(
    patch: &IrPatch,
    baseline_snapshot_hash: &str,
    baseline: &LogicalCircuitSnapshot,
    observed: &Value,
    fusion: &FusionView,
) -> MaterializationPlan {
    let mut steps = Vec::with_capacity(patch.operations.len());
    let mut risks: Vec<String> = Vec::new();
    let mut rollback_warnings: Vec<String> = Vec::new();
    let mut diff = Vec::with_capacity(patch.operations.len());
    let mut affected = std::collections::BTreeSet::new();
    let mut rollbackable = true;

    // 折叠补丁全部操作得到目标网络/元件引脚集：新增网络的成员与挂接判定面向
    // 补丁后的目标快照，而不是基线（成员可能来自同一补丁新增或改名的对象）。
    let mut effective_nets: std::collections::BTreeSet<String> =
        baseline.nets.iter().map(|net| net.id.clone()).collect();
    // 新网络名 → 改名前的旧网络 ID：挂接锚点需按旧名在基线观测中定位。
    let mut renames: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    let mut effective_pins: std::collections::BTreeMap<String, Vec<String>> = baseline
        .components
        .iter()
        .map(|component| {
            (component.id.clone(), component.pins.iter().map(|pin| pin.id.clone()).collect())
        })
        .collect();
    for operation in &patch.operations {
        match operation {
            circuitfabric_contracts::PatchOperation::AddComponent { reference, pins, .. } => {
                effective_pins.insert(
                    format!("component:{reference}"),
                    pins.iter().map(|pin| pin.id.clone()).collect(),
                );
            }
            circuitfabric_contracts::PatchOperation::RemoveComponent { component_id } => {
                effective_pins.remove(component_id);
            }
            // 改名后旧 ID 消失：同补丁内按新名称挂接网络是显式加入，不是新建。
            circuitfabric_contracts::PatchOperation::RenameNet { net_id, name } => {
                effective_nets.remove(net_id);
                effective_nets.insert(format!("net:{name}"));
                renames.insert(name.clone(), net_id.clone());
            }
            _ => {}
        }
    }
    let observed_records = observed_components(observed);

    for (index, operation) in patch.operations.iter().enumerate() {
        match operation {
            circuitfabric_contracts::PatchOperation::SetComponentValue { component_id, value } => {
                let component = baseline.components.iter().find(|c| c.id == *component_id);
                let mapping = fusion.mapping_for(component_id);
                let stable = mapping.is_some_and(|m| {
                    m.status.is_stable() && m.present_in_latest && m.primitive_ids.len() == 1
                });
                affected.insert(component_id.clone());
                diff.push(PlanDiffEntry {
                    kind: DiffKind::Modify,
                    subject: component_id.clone(),
                    detail: format!("元件 {component_id} 属性值 → {value}"),
                });
                steps.push(PlanStep {
                    index: index as u32,
                    kind: StepKind::SetComponentValue,
                    logical_id: component_id.clone(),
                    external_primitive_ids: mapping
                        .map_or_else(Vec::new, |m| m.primitive_ids.clone()),
                    spec: Value::Null,
                    preconditions: vec![Precondition {
                        description: "元件存在、原值可回读且身份映射稳定。".into(),
                        satisfied: component.is_some_and(|c| c.value.is_some()) && stable,
                    }],
                    rollback: RollbackSpec {
                        kind: RollbackKind::RestoreProperty,
                        data: json!({"value": component.and_then(|c| c.value.clone())}),
                    },
                });
            }
            circuitfabric_contracts::PatchOperation::RenameNet { net_id, name } => {
                let net = baseline.nets.iter().find(|n| n.id == *net_id);
                let mapping = fusion.mapping_for(net_id);
                let stable = mapping.is_some_and(|m| {
                    m.status.is_stable() && m.present_in_latest && !m.primitive_ids.is_empty()
                });
                affected.insert(net_id.clone());
                diff.push(PlanDiffEntry {
                    kind: DiffKind::Modify,
                    subject: net_id.clone(),
                    detail: format!("网络 {net_id} 的全部导线网络属性 → {name}"),
                });
                steps.push(PlanStep {
                    index: index as u32,
                    kind: StepKind::RenameNet,
                    logical_id: net_id.clone(),
                    external_primitive_ids: mapping
                        .map_or_else(Vec::new, |m| m.primitive_ids.clone()),
                    spec: Value::Null,
                    preconditions: vec![Precondition {
                        description: "网络与导线映射稳定，目标名称非空且不会合并已有网络。".into(),
                        satisfied: stable
                            && net.is_some_and(|n| n.name.is_some())
                            && !name.trim().is_empty()
                            && !baseline.nets.iter().any(|n| n.name.as_ref() == Some(name)),
                    }],
                    rollback: RollbackSpec {
                        kind: RollbackKind::RestoreProperty,
                        data: json!({"name": net.and_then(|n| n.name.clone())}),
                    },
                });
            }
            circuitfabric_contracts::PatchOperation::AddComponent {
                reference,
                value,
                pins,
                clone_from,
            } => {
                // 逻辑 ID 按观测约定由系统派生，模型不提供快照内部字段。
                let logical_id = format!("component:{reference}");
                // 放置来源：clone_from 指向的观测位号，复用其库件身份与姿态。
                let source =
                    clone_from.as_deref().and_then(|designator| observed_records.get(designator));
                let placeable_source = source.is_some_and(|record| placeable(record));
                if !placeable_source {
                    risks.push(format!(
                        "元件 {logical_id} 缺少可用的 clone_from 观测位号（需指向含库身份与放置的元件），无法物化放置，计划不可批准。"
                    ));
                }
                risks.push(format!(
                    "元件 {logical_id} 放置使用官方 BETA create/modify API，真实客户端行为仍需验证。"
                ));
                let conflicts = baseline.components.iter().any(|c| c.id == logical_id);
                if conflicts {
                    risks.push(format!("元件 {logical_id} 已存在于基线，禁止重复新增。"));
                }
                let spec = source
                    .filter(|_| placeable_source)
                    .map(|record| placement_spec(record, reference, value.as_deref()))
                    .unwrap_or(Value::Null);
                affected.insert(logical_id.clone());
                diff.push(PlanDiffEntry {
                    kind: DiffKind::Add,
                    subject: logical_id.clone(),
                    detail: format!(
                        "新增元件 {reference}（值 {}，引脚 {}{}）。",
                        value.as_deref().unwrap_or("（未标注）"),
                        pins.len(),
                        clone_from
                            .as_deref()
                            .map_or_else(String::new, |source| format!("，放置复用 {source} 库件"))
                    ),
                });
                steps.push(PlanStep {
                    index: index as u32,
                    kind: StepKind::PlaceComponent,
                    logical_id,
                    external_primitive_ids: Vec::new(),
                    spec,
                    preconditions: vec![
                        Precondition {
                            description: "放置来源可用（clone_from 指向含库身份的观测元件）。"
                                .to_owned(),
                            satisfied: placeable_source,
                        },
                        Precondition {
                            description: "引用不与基线元件冲突。".to_owned(),
                            satisfied: !conflicts,
                        },
                    ],
                    rollback: RollbackSpec {
                        kind: RollbackKind::DeleteCreated,
                        data: json!({"designator": reference}),
                    },
                });
            }
            circuitfabric_contracts::PatchOperation::RemoveComponent { component_id } => {
                // 补偿数据来自基线观测的库身份与放置。回滚能力只提示、不阻止批准：
                // 立创编号可由执行器核验后重建；仅有原库身份时只能未经核验直接重建。
                let designator = component_id.strip_prefix("component:").unwrap_or(component_id);
                let record = observed_records.get(designator);
                let compensatable = record.is_some_and(|record| placeable(record));
                if !compensatable {
                    rollbackable = false;
                    let warning = format!(
                        "删除 {designator} 后无法自动回滚：基线观测缺少库身份/立创编号或放置数据，恢复只能手动重新放置。"
                    );
                    risks.push(warning.clone());
                    rollback_warnings.push(warning);
                } else if let Some(supplier) = record
                    .and_then(|record| record.get("supplierId"))
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                {
                    risks.push(format!(
                        "删除 {designator} 的回滚按立创编号 {supplier} 从库中重建同型号器件（可能是新的工程副本），需回读核对。"
                    ));
                } else {
                    let warning = format!(
                        "{designator} 没有立创编号，删除后的回滚只能直接用原库身份重建，事先无法核验，可能失败或结果未知。"
                    );
                    risks.push(warning.clone());
                    rollback_warnings.push(warning);
                }
                let baseline_component =
                    baseline.components.iter().find(|component| component.id == *component_id);
                let mapping = fusion.mapping_for(component_id);
                let mapping_stable = mapping
                    .map(|mapping| {
                        mapping.status == MappingStatus::Stable && mapping.present_in_latest
                    })
                    .unwrap_or(false);
                if !mapping_stable {
                    risks.push(format!(
                        "身份映射不稳定：{component_id} 的图元映射缺失、漂移或存在冲突，delete_component 可能误删。"
                    ));
                }
                affected.insert(component_id.clone());
                diff.push(PlanDiffEntry {
                    kind: DiffKind::Remove,
                    subject: component_id.clone(),
                    detail: format!("删除元件 {component_id} 及其引脚成员关系。"),
                });
                steps.push(PlanStep {
                    index: index as u32,
                    kind: StepKind::DeleteComponent,
                    logical_id: component_id.clone(),
                    external_primitive_ids: mapping
                        .map_or_else(Vec::new, |m| m.primitive_ids.clone()),
                    spec: json!({"designator": designator}),
                    preconditions: vec![
                        Precondition {
                            description: format!("逻辑 ID {component_id} 存在于基线快照。"),
                            satisfied: baseline_component.is_some(),
                        },
                        Precondition {
                            description: "身份映射稳定（无漂移/冲突，且存在于最新观测）。"
                                .to_owned(),
                            satisfied: mapping_stable,
                        },
                    ],
                    rollback: RollbackSpec {
                        kind: if compensatable {
                            RollbackKind::RestoreComponent
                        } else {
                            RollbackKind::None
                        },
                        // 补偿数据 = 外部观测记录（库身份 + 放置 + 原值）。
                        data: record
                            .cloned()
                            .or_else(|| {
                                baseline_component
                                    .and_then(|component| serde_json::to_value(component).ok())
                            })
                            .unwrap_or(Value::Null),
                    },
                });
            }
            circuitfabric_contracts::PatchOperation::AddNet { name, pins } => {
                // 物化 = 逐脚连线（挂到既有网络或链成新网络）；回滚 = 删除新建导线。
                let logical_id = format!("net:{name}");
                let joins_existing = effective_nets.contains(&logical_id);
                risks.push(format!(
                    "网络 {logical_id} 以引脚连线物化（官方 BETA create API），回滚为删除新建导线。"
                ));
                let missing: Vec<String> = pins
                    .iter()
                    .filter(|pin| {
                        !effective_pins
                            .get(&pin.component_id)
                            .is_some_and(|component_pins| component_pins.contains(&pin.pin_id))
                    })
                    .map(|pin| format!("{}:{}", pin.component_id, pin.pin_id))
                    .collect();
                if !missing.is_empty() {
                    risks.push(format!(
                        "网络 {logical_id} 引用了目标快照中不存在的引脚：{}。",
                        missing.join(", ")
                    ));
                }
                affected.insert(logical_id.clone());
                diff.push(PlanDiffEntry {
                    kind: DiffKind::Add,
                    subject: logical_id.clone(),
                    detail: if joins_existing {
                        format!("向既有网络 {logical_id} 显式挂接成员 {} 个。", pins.len())
                    } else {
                        format!("新增网络 {logical_id}（成员 {} 个）。", pins.len())
                    },
                });
                steps.push(PlanStep {
                    index: index as u32,
                    kind: StepKind::AddNet,
                    logical_id,
                    external_primitive_ids: Vec::new(),
                    spec: wire_connections(name, pins, observed, baseline, &renames),
                    preconditions: vec![Precondition {
                        description: "网络成员引用的元件引脚均存在于目标快照。".to_owned(),
                        satisfied: missing.is_empty(),
                    }],
                    rollback: RollbackSpec { kind: RollbackKind::DeleteCreated, data: json!({}) },
                });
            }
            circuitfabric_contracts::PatchOperation::AnnotateSemantic { key, value } => {
                // 纯 IR 标注：不产生 EDA 写入。
                let subject = format!("semantics:{key}");
                diff.push(PlanDiffEntry {
                    kind: DiffKind::Note,
                    subject: subject.clone(),
                    detail: format!("语义标注 {key}={value}（仅 IR，无 EDA 写入）。"),
                });
                steps.push(PlanStep {
                    index: index as u32,
                    kind: StepKind::AnnotateOnly,
                    logical_id: subject,
                    external_primitive_ids: Vec::new(),
                    spec: Value::Null,
                    preconditions: vec![Precondition {
                        description: "纯 IR 标注，不涉及 EDA 写入。".to_owned(),
                        satisfied: true,
                    }],
                    rollback: RollbackSpec { kind: RollbackKind::None, data: json!({}) },
                });
            }
        }
    }

    let mut plan = MaterializationPlan {
        schema_version: 1,
        backend: PLAN_BACKEND.to_owned(),
        project_id: String::new(),
        session_id: String::new(),
        document: Value::Null,
        patch: Some(patch.clone()),
        baseline_snapshot_hash: baseline_snapshot_hash.to_owned(),
        plan_hash: String::new(),
        created_from_patch_rationale: patch.rationale.clone(),
        steps,
        affected_logical_ids: affected.into_iter().collect(),
        risks,
        rollbackable,
        diff,
        rollback_warnings,
    };
    plan.plan_hash = plan_digest(&plan);
    plan
}

fn map_io(error: crate::observation::ObservationError, path: &Path) -> PlanError {
    match error {
        crate::observation::ObservationError::CreateDirectory { path, source }
        | crate::observation::ObservationError::Write { path, source } => {
            PlanError::Write { path, source }
        }
        crate::observation::ObservationError::Commit { from, to, source } => {
            PlanError::Commit { from, to, source }
        }
        other => PlanError::Write {
            path: path.to_path_buf(),
            source: std::io::Error::other(other.to_string()),
        },
    }
}

/// 把物化计划原子写入 `<项目根>/observations/plan-<planHash 去前缀>.json`。
///
/// # Errors
///
/// 返回 [`PlanError`] 当目录创建、写入或提交失败。
pub fn persist_plan(project_root: &Path, plan: &MaterializationPlan) -> Result<PathBuf, PlanError> {
    let path = plan_path(project_root, &plan.plan_hash);
    let bytes = serde_json::to_vec_pretty(plan).expect("计划序列化不应失败");
    write_atomic(&path, &bytes).map_err(|error| map_io(error, &path))?;
    Ok(path)
}

/// 读取物化计划；文件不存在时返回 `Ok(None)`。
///
/// # Errors
///
/// 返回 [`PlanError`] 当读取或解析失败。
pub fn load_plan(
    project_root: &Path,
    plan_hash: &str,
) -> Result<Option<MaterializationPlan>, PlanError> {
    let path = plan_path(project_root, plan_hash);
    let raw = match std::fs::read(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(PlanError::Read { path, source }),
    };
    serde_json::from_slice(&raw).map(Some).map_err(|source| PlanError::Parse { path, source })
}

#[cfg(test)]
mod tests {
    use circuitfabric_contracts::{Component, IrPatch, PatchOperation, Pin, PinRef};

    use super::*;

    fn component(id: &str, reference: &str) -> Component {
        Component {
            id: id.to_owned(),
            reference: reference.to_owned(),
            value: Some("10k".to_owned()),
            pins: vec![Pin { id: "1".to_owned(), name: "A".to_owned() }],
            evidence: Vec::new(),
        }
    }

    fn baseline() -> LogicalCircuitSnapshot {
        let mut snapshot =
            LogicalCircuitSnapshot::empty(circuitfabric_contracts::SnapshotAuthority::Observed);
        snapshot.components.push(component("component:R1", "R1"));
        snapshot.components.push(component("component:R2", "R2"));
        snapshot.snapshot_hash = "sha256:baseline".to_owned();
        snapshot
    }

    /// 与 baseline() 对应的观测载荷：含库身份与放置，可作放置来源/删除补偿。
    fn observed_payload() -> Value {
        json!({
            "components": [
                {"primitiveId":"prim-r1","designator":"R1","name":"10k","libraryUuid":"lib-1",
                 "componentUuid":"cmp-r1","x":0.0,"y":0.0,"rotation":0.0,"mirror":false},
                {"primitiveId":"prim-r2","designator":"R2","name":"10k","libraryUuid":"lib-1",
                 "componentUuid":"cmp-r2","x":100.0,"y":40.0,"rotation":90.0,"mirror":true}
            ],
            "wires": [], "nets": []
        })
    }

    fn patch(operations: Vec<PatchOperation>) -> IrPatch {
        IrPatch {
            base_snapshot_hash: "sha256:baseline".to_owned(),
            operations,
            rationale: "测试补丁".to_owned(),
        }
    }

    fn plan_for(operations: Vec<PatchOperation>) -> MaterializationPlan {
        build_plan(
            &patch(operations),
            "sha256:baseline",
            &baseline(),
            &observed_payload(),
            &FusionView { mappings: Vec::new(), conflicts: Vec::new() },
        )
    }

    #[test]
    fn same_baseline_and_patch_yield_the_same_plan_hash() {
        let operations = vec![
            PatchOperation::AddComponent {
                reference: "R3".to_owned(),
                value: Some("10k".to_owned()),
                pins: vec![Pin { id: "1".to_owned(), name: "A".to_owned() }],
                clone_from: Some("R1".to_owned()),
            },
            PatchOperation::RemoveComponent { component_id: "component:R1".to_owned() },
            PatchOperation::AddNet {
                name: "VCC".to_owned(),
                pins: vec![PinRef {
                    component_id: "component:R2".to_owned(),
                    pin_id: "1".to_owned(),
                }],
            },
            PatchOperation::AnnotateSemantic {
                key: "domain".to_owned(),
                value: "power".to_owned(),
            },
        ];
        let first = plan_for(operations.clone());
        let second = plan_for(operations);
        assert_eq!(first.plan_hash, second.plan_hash);
        assert!(first.plan_hash.starts_with("sha256:"));
        // 哈希覆盖除自身外的内容：改基线即变。
        let moved = build_plan(
            &patch(vec![]),
            "sha256:other",
            &baseline(),
            &observed_payload(),
            &FusionView { mappings: Vec::new(), conflicts: Vec::new() },
        );
        assert_ne!(first.plan_hash, moved.plan_hash);
    }

    #[test]
    fn different_patches_yield_different_plan_hashes() {
        let add = plan_for(vec![PatchOperation::AddComponent {
            reference: "R3".to_owned(),
            value: Some("10k".to_owned()),
            pins: vec![Pin { id: "1".to_owned(), name: "A".to_owned() }],
            clone_from: Some("R1".to_owned()),
        }]);
        let annotate = plan_for(vec![PatchOperation::AnnotateSemantic {
            key: "domain".to_owned(),
            value: "power".to_owned(),
        }]);
        assert_ne!(add.plan_hash, annotate.plan_hash);
        let changed_value = plan_for(vec![PatchOperation::AddComponent {
            reference: "R3".to_owned(),
            value: Some("47k".to_owned()),
            pins: vec![Pin { id: "1".to_owned(), name: "A".to_owned() }],
            clone_from: Some("R1".to_owned()),
        }]);
        assert_ne!(add.plan_hash, changed_value.plan_hash, "审批必须覆盖补丁实际内容");
    }

    #[test]
    fn step_mapping_preconditions_and_rollback_rules() {
        let plan = plan_for(vec![
            PatchOperation::AddComponent {
                reference: "R3".to_owned(),
                value: Some("10k".to_owned()),
                pins: vec![Pin { id: "1".to_owned(), name: "A".to_owned() }],
                clone_from: None,
            },
            PatchOperation::RemoveComponent { component_id: "component:R1".to_owned() },
            PatchOperation::RemoveComponent { component_id: "component:GHOST".to_owned() },
            PatchOperation::AddNet {
                name: "VCC".to_owned(),
                pins: vec![PinRef {
                    component_id: "component:R2".to_owned(),
                    pin_id: "1".to_owned(),
                }],
            },
        ]);

        assert_eq!(plan.backend, PLAN_BACKEND);
        assert_eq!(plan.baseline_snapshot_hash, "sha256:baseline");
        assert_eq!(plan.steps.len(), 4);
        assert_eq!(plan.steps[0].kind, StepKind::PlaceComponent);
        assert!(
            !plan.steps[0].preconditions[0].satisfied,
            "clone_from 缺失 → 放置来源前置条件不满足"
        );
        assert_eq!(plan.steps[0].rollback.kind, RollbackKind::DeleteCreated);

        // 观测含库身份与放置 → 删除可补偿重建；无融合映射时身份映射前置条件仍不满足。
        assert_eq!(plan.steps[1].kind, StepKind::DeleteComponent);
        assert!(
            plan.steps[1].preconditions.iter().any(|p| !p.satisfied),
            "无融合映射 → 身份映射稳定前置条件不满足"
        );
        assert_eq!(plan.steps[1].rollback.kind, RollbackKind::RestoreComponent);
        assert_eq!(plan.steps[1].rollback.data["designator"], "R1");
        assert_eq!(plan.steps[1].rollback.data["libraryUuid"], "lib-1");
        assert_eq!(plan.steps[1].spec["designator"], "R1");

        // 基线快照中不存在的逻辑 ID：标前置条件失败而不是整体拒绝，且不可补偿。
        assert_eq!(plan.steps[2].logical_id, "component:GHOST");
        assert!(
            plan.steps[2]
                .preconditions
                .iter()
                .any(|p| p.description.contains("基线快照") && !p.satisfied)
        );
        assert_eq!(plan.steps[2].rollback.kind, RollbackKind::None);

        assert_eq!(plan.steps[3].kind, StepKind::AddNet);
        assert_eq!(plan.steps[3].rollback.kind, RollbackKind::DeleteCreated);
        assert!(plan.risks.iter().any(|risk| risk.contains("回滚为删除新建导线")));
        assert!(!plan.rollbackable, "GHOST 删除不可补偿 → 计划整体不可安全回滚");
        assert!(!plan.preconditions_satisfied());

        // diff 与 affected 顺序确定。
        assert_eq!(
            plan.affected_logical_ids,
            vec!["component:GHOST", "component:R1", "component:R3", "net:VCC"]
        );
    }

    #[test]
    fn structural_specs_carry_placement_and_wire_anchors() {
        // 未命名地网经导线连接；同补丁内先改名再挂接，锚点按旧名回溯到导线端点。
        let mut base = baseline();
        base.nets.push(circuitfabric_contracts::Net {
            id: "net:".to_owned(),
            name: None,
            pins: vec![PinRef { component_id: "component:R2".to_owned(), pin_id: "1".to_owned() }],
        });
        let mut observed = observed_payload();
        observed["wires"] =
            json!([{ "primitiveId": "w1", "net": "", "points": [[5, 5], [10, 5]] }]);
        let plan = build_plan(
            &patch(vec![
                PatchOperation::RenameNet { net_id: "net:".to_owned(), name: "GND".to_owned() },
                PatchOperation::AddNet {
                    name: "GND".to_owned(),
                    pins: vec![PinRef {
                        component_id: "component:R1".to_owned(),
                        pin_id: "1".to_owned(),
                    }],
                },
                PatchOperation::AddNet {
                    name: "BS".to_owned(),
                    pins: vec![
                        PinRef { component_id: "component:R1".to_owned(), pin_id: "1".to_owned() },
                        PinRef { component_id: "component:R2".to_owned(), pin_id: "1".to_owned() },
                    ],
                },
                PatchOperation::AddComponent {
                    reference: "J1".to_owned(),
                    value: Some("HOST_SPI_8P".to_owned()),
                    pins: vec![Pin { id: "1".to_owned(), name: "VCC".to_owned() }],
                    clone_from: Some("R2".to_owned()),
                },
            ]),
            "sha256:baseline",
            &base,
            &observed,
            &FusionView { mappings: Vec::new(), conflicts: Vec::new() },
        );

        // 挂接既有网络：锚点为旧名导线的端点坐标。
        let gnd = &plan.steps[1];
        assert_eq!(gnd.spec["net"], "GND");
        assert_eq!(gnd.spec["connections"][0]["anchor"], json!({"x": 5, "y": 5}));
        assert_eq!(gnd.spec["connections"][0]["pin"], json!({"designator": "R1", "pin": "1"}));

        // 新网络：首脚短桩，后续链到前一脚。
        let bs = &plan.steps[2];
        assert_eq!(bs.spec["connections"][0]["anchor"], json!({"stub": true}));
        assert_eq!(
            bs.spec["connections"][1]["anchor"],
            json!({"pin": {"designator": "R1", "pin": "1"}})
        );

        // 放置规格：复用来源库身份，位置取源旁偏移，姿态随源。
        let place = &plan.steps[3];
        assert_eq!(place.spec["libraryUuid"], "lib-1");
        assert_eq!(place.spec["componentUuid"], "cmp-r2");
        assert_eq!(place.spec["designator"], "J1");
        assert_eq!(place.spec["name"], "HOST_SPI_8P");
        assert_eq!(place.spec["x"], 300.0);
        assert_eq!(place.spec["y"], 240.0);
        assert_eq!(place.spec["rotation"], 90.0);
        assert_eq!(place.spec["mirror"], true);
    }

    #[test]
    fn rollback_capability_warns_without_blocking_approval() {
        // R1 有立创编号：按编号核验重建，只有说明，无提醒；R2 只有原库身份：醒目提醒。
        let mut observed = observed_payload();
        observed["components"][0]["supplierId"] = json!("C25744");
        let plan = build_plan(
            &patch(vec![
                PatchOperation::RemoveComponent { component_id: "component:R1".to_owned() },
                PatchOperation::RemoveComponent { component_id: "component:R2".to_owned() },
            ]),
            "sha256:baseline",
            &baseline(),
            &observed,
            &FusionView { mappings: Vec::new(), conflicts: Vec::new() },
        );
        assert_eq!(plan.steps[0].rollback.data["supplierId"], "C25744");
        assert!(plan.risks.iter().any(|risk| risk.contains("C25744")));
        assert_eq!(plan.rollback_warnings.len(), 1);
        assert!(plan.rollback_warnings[0].contains("R2"));
        assert!(plan.rollbackable);
        assert!(
            plan.steps.iter().all(|step| step
                .preconditions
                .iter()
                .all(|p| p.satisfied || p.description.contains("身份映射"))),
            "回滚能力不再是批准前置条件"
        );

        // 没有提醒时不序列化该字段，旧计划哈希保持不变。
        let quiet = plan_for(vec![PatchOperation::AnnotateSemantic {
            key: "domain".to_owned(),
            value: "power".to_owned(),
        }]);
        assert!(serde_json::to_value(&quiet).unwrap().get("rollbackWarnings").is_none());
    }

    #[test]
    fn supplier_id_alone_is_enough_to_compensate_a_delete() {
        let mut observed = observed_payload();
        for field in ["libraryUuid", "componentUuid"] {
            observed["components"][0][field] = Value::Null;
        }
        observed["components"][0]["supplierId"] = json!("C25744");
        let plan = build_plan(
            &patch(vec![PatchOperation::RemoveComponent {
                component_id: "component:R1".to_owned(),
            }]),
            "sha256:baseline",
            &baseline(),
            &observed,
            &FusionView { mappings: Vec::new(), conflicts: Vec::new() },
        );
        assert_eq!(plan.steps[0].rollback.kind, RollbackKind::RestoreComponent);
        assert!(plan.rollback_warnings.is_empty());
        assert!(plan.rollbackable);
    }

    #[test]
    fn uncompensatable_sources_warn_and_block_placement() {
        // 观测缺库身份：删除不可补偿（只提醒）；clone_from 指向缺失位号：放置不可满足。
        let mut observed = observed_payload();
        for field in ["libraryUuid", "componentUuid"] {
            observed["components"][0][field] = Value::Null;
        }
        let plan = build_plan(
            &patch(vec![
                PatchOperation::RemoveComponent { component_id: "component:R1".to_owned() },
                PatchOperation::AddComponent {
                    reference: "J1".to_owned(),
                    value: None,
                    pins: vec![Pin { id: "1".to_owned(), name: "A".to_owned() }],
                    clone_from: Some("GHOST".to_owned()),
                },
            ]),
            "sha256:baseline",
            &baseline(),
            &observed,
            &FusionView { mappings: Vec::new(), conflicts: Vec::new() },
        );
        assert_eq!(plan.steps[0].rollback.kind, RollbackKind::None);
        assert!(plan.steps[0].preconditions.iter().all(|p| !p.description.contains("补偿")));
        assert!(!plan.steps[1].preconditions[0].satisfied, "clone_from 不可解析 → 不可放置");
        assert!(!plan.rollbackable);
        assert!(!plan.preconditions_satisfied());
        assert!(plan.rollback_warnings.iter().any(|w| w.contains("R1") && w.contains("无法自动回滚")));
        assert!(plan.risks.iter().any(|risk| risk.contains("clone_from")));
    }

    #[test]
    fn add_net_membership_resolves_against_the_post_patch_snapshot() {
        let mut base = baseline();
        base.nets.push(circuitfabric_contracts::Net {
            id: "net:VCC".to_owned(),
            name: Some("VCC".to_owned()),
            pins: Vec::new(),
        });
        let plan = build_plan(
            &patch(vec![
                PatchOperation::AddComponent {
                    reference: "J1".to_owned(),
                    value: Some("HOST_SPI_8P".to_owned()),
                    pins: vec![Pin { id: "1".to_owned(), name: "VCC".to_owned() }],
                    clone_from: Some("R1".to_owned()),
                },
                PatchOperation::RemoveComponent { component_id: "component:R1".to_owned() },
                PatchOperation::AddNet {
                    name: "BS".to_owned(),
                    pins: vec![
                        PinRef { component_id: "component:J1".to_owned(), pin_id: "1".to_owned() },
                        PinRef { component_id: "component:R2".to_owned(), pin_id: "9".to_owned() },
                        PinRef { component_id: "component:R1".to_owned(), pin_id: "1".to_owned() },
                    ],
                },
                PatchOperation::AddNet {
                    name: "VCC".to_owned(),
                    pins: vec![PinRef {
                        component_id: "component:J1".to_owned(),
                        pin_id: "1".to_owned(),
                    }],
                },
            ]),
            "sha256:baseline",
            &base,
            &observed_payload(),
            &FusionView { mappings: Vec::new(), conflicts: Vec::new() },
        );

        // 同补丁新增的 J1 引脚视为存在；基线没有的引脚号与被删除元件的引脚都标记缺失。
        let risk = plan.risks.iter().find(|risk| risk.contains("不存在的引脚")).expect("风险存在");
        assert!(risk.contains("component:R2:9"), "{risk}");
        assert!(risk.contains("component:R1:1"), "{risk}");
        assert!(!risk.contains("component:J1:1"), "{risk}");
        let step = &plan.steps[2];
        assert!(!step.preconditions[0].satisfied, "成员引用不完整时前置条件不满足");

        // 同名既有网络是显式挂接成员，不是新建。
        let join = plan.diff.iter().find(|entry| entry.subject == "net:VCC").expect("VCC diff");
        assert!(join.detail.contains("挂接"), "{}", join.detail);
    }

    #[test]
    fn stable_fusion_mapping_satisfies_delete_precondition() {
        let fusion = crate::fusion::build_fusion_view(&[crate::fusion::InspectObservation {
            snapshot_hash: "sha256:baseline".to_owned(),
            received_at: "2026-10-22T08:00:00.000Z".to_owned(),
            external_refs: serde_json::json!([
                { "logicalId": "component:R1", "primitiveId": "prim-r1", "pageUuid": "page-1" },
            ]),
        }]);
        let plan = build_plan(
            &patch(vec![PatchOperation::RemoveComponent {
                component_id: "component:R1".to_owned(),
            }]),
            "sha256:baseline",
            &baseline(),
            &observed_payload(),
            &fusion,
        );
        let step = &plan.steps[0];
        assert_eq!(step.external_primitive_ids, vec!["prim-r1"]);
        assert!(
            step.preconditions.iter().all(|p| p.satisfied),
            "身份映射稳定且观测含补偿数据 → 删除可批准"
        );
        assert!(plan.preconditions_satisfied());
        assert!(plan.rollbackable);
    }

    #[test]
    fn annotate_only_is_a_note_without_eda_write() {
        let plan = plan_for(vec![PatchOperation::AnnotateSemantic {
            key: "domain".to_owned(),
            value: "power".to_owned(),
        }]);
        assert_eq!(plan.steps[0].kind, StepKind::AnnotateOnly);
        assert_eq!(plan.diff[0].kind, super::DiffKind::Note);
        assert!(plan.rollbackable);
        assert!(plan.preconditions_satisfied());
        assert!(plan.affected_logical_ids.is_empty());
    }

    #[test]
    fn persist_and_load_roundtrip() {
        let dir = std::env::temp_dir().join(format!(
            "circuitfabric-plan-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos()
        ));
        let root = dir.join("project");
        let plan = plan_for(vec![PatchOperation::AddComponent {
            reference: "R3".to_owned(),
            value: Some("10k".to_owned()),
            pins: vec![Pin { id: "1".to_owned(), name: "A".to_owned() }],
            clone_from: Some("R1".to_owned()),
        }]);
        let path = persist_plan(&root, &plan).expect("落盘计划");
        assert!(path.is_file());
        let loaded = load_plan(&root, &plan.plan_hash).expect("读取计划").expect("计划存在");
        assert_eq!(loaded.plan_hash, plan.plan_hash);
        assert_eq!(loaded.steps.len(), 1);
        assert_eq!(loaded.steps[0].spec["designator"], "R3", "物化规格随计划持久化");
        assert!(load_plan(&root, "sha256:missing").expect("读取计划").is_none());

        std::fs::remove_dir_all(&dir).ok();
    }
}
