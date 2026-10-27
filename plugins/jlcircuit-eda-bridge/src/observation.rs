//! Bridge 侧 inspect 观测载荷 → observed snapshot 的构建与持久化。
//!
//! 实现 `docs/bridge-operations-protocol.zh-CN.md` 第 4–6 节：
//! - 逻辑身份为 `component:<designator>` / `net:<name>`，引脚用引脚号；EDA 图元
//!   ID 只进入外部引用映射，绝不成为逻辑身份。
//! - 元件按位号、引脚按引脚号、网络按名称、成员按 `元件:引脚` 排序后构建，
//!   同一原理图两次 inspect 得到相同哈希。
//! - `logicalHash`/`snapshotHash` 由 `circuitfabric-core` 的 `stamp_snapshot` 计算；
//!   `physicalHash` 对排序后的物理观测投影做 `sha256:` 摘要。
//! - 观测记录、DRC 报告、视觉证据持久化到 `<项目根>/observations/`，原子写。

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use circuitfabric_contracts::{
    Component, ConstraintResult, FactStatus, LogicalCircuitSnapshot, Net, Pin, PinRef, Severity,
    SnapshotAuthority,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// 观测记录目录名（位于项目根下）。
pub const OBSERVATIONS_DIRECTORY: &str = "observations";

/// 观测面标识（`inconclusive` 取值），见协议文档第 4 节。
pub mod observation_face {
    pub const DOCUMENT: &str = "document";
    pub const COMPONENTS: &str = "components";
    pub const PINS: &str = "pins";
    pub const PHYSICAL_PLACEMENT: &str = "physicalPlacement";
    pub const WIRES: &str = "wires";
    pub const NETS: &str = "nets";
    pub const BUSES: &str = "buses";
    pub const NET_MEMBERSHIP: &str = "netMembership";
    pub const DRC: &str = "drc";
    pub const CANVAS_IMAGE: &str = "canvasImage";
    pub const ENVIRONMENT: &str = "environment";
}

#[derive(Debug, Error)]
pub enum ObservationError {
    #[error("观测目录无法创建（{path}）：{source}")]
    CreateDirectory { path: PathBuf, source: std::io::Error },
    #[error("观测记录写入失败（{path}）：{source}")]
    Write { path: PathBuf, source: std::io::Error },
    #[error("观测记录提交失败（{from} -> {to}）：{source}")]
    Commit { from: PathBuf, to: PathBuf, source: std::io::Error },
    #[error("观测记录读取失败（{path}）：{source}")]
    Read { path: PathBuf, source: std::io::Error },
    #[error("观测记录不是有效 JSON（{path}）：{source}")]
    Parse { path: PathBuf, source: serde_json::Error },
    #[error("快照盖章失败：{0}")]
    Stamp(#[from] circuitfabric_core::CoreError),
}

/// 从 inspect 观测载荷构建出的完整观测结果。
#[derive(Clone, Debug)]
pub struct ObservationBuild {
    /// 已盖章的 observed snapshot（含 `parent_snapshot_hash` 链）。
    pub snapshot: LogicalCircuitSnapshot,
    /// 物理观测投影的 `sha256:` 摘要（已写入 `snapshot.physical_hash`）。
    pub physical_hash: String,
    /// 排序后的物理投影（随观测记录持久化）。
    pub physical_projection: Value,
    /// 逻辑 ID → EDA 图元 ID + 页 UUID 的外部引用映射。
    pub external_refs: Value,
    /// 缺失/不可用的观测面（排序去重）。
    pub inconclusive: Vec<String>,
    /// `validate_snapshot` 结果；`inconclusive` 非空时追加一条观测层记录。
    pub validation: Vec<ConstraintResult>,
    /// 可解释的采集问题（按原因归并，例子有上限，不包含原始图元 ID）。
    pub diagnostics: Vec<Value>,
}

struct ObservedPin {
    number: String,
    name: String,
    x: Option<f64>,
    y: Option<f64>,
}

impl ObservedPin {
    fn projection_json(&self) -> Value {
        json!({ "number": self.number, "x": self.x, "y": self.y })
    }
}

struct ObservedComponent {
    designator: String,
    value: Option<String>,
    primitive_id: Option<String>,
    footprint: Option<String>,
    x: Option<f64>,
    y: Option<f64>,
    rotation: Option<f64>,
    mirror: bool,
    page_uuid: Option<String>,
    pins: Vec<ObservedPin>,
}

struct ObservedWire {
    primitive_id: Option<String>,
    net: Option<String>,
    points: Vec<[f64; 2]>,
}

/// 计算字节的 `sha256:` 内容摘要。
#[must_use]
pub fn sha256_digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

/// 把 unix 毫秒格式化为 RFC 3339 UTC（`YYYY-MM-DDTHH:MM:SS.mmmZ`）。
#[must_use]
pub fn rfc3339_millis(unix_milliseconds: u64) -> String {
    let seconds = unix_milliseconds / 1000;
    let millis = unix_milliseconds % 1000;
    let base = circuitfabric_project::rfc3339(seconds);
    format!("{}.{millis:03}Z", base.trim_end_matches('Z'))
}

/// 从执行器返回的 inspect 观测载荷构建 observed snapshot。
///
/// # Errors
///
/// 返回 [`ObservationError::Stamp`] 当快照无法序列化盖章。
///
/// # Panics
///
/// 物理投影序列化为 JSON 不应失败；失败即为编程错误，直接 panic。
pub fn build_observation(
    payload: &Value,
    parent_snapshot_hash: Option<&str>,
) -> Result<ObservationBuild, ObservationError> {
    let document = payload.get(observation_face::DOCUMENT).cloned().unwrap_or(Value::Null);
    let page_uuid = document.get("pageUuid").and_then(Value::as_str).map(str::to_owned);
    let mut inconclusive = BTreeSet::new();
    if let Some(faces) = payload.get("inconclusive").and_then(Value::as_array) {
        inconclusive.extend(faces.iter().filter_map(Value::as_str).map(str::to_owned));
    }
    if document.get("kind").and_then(Value::as_str) != Some("schematic")
        || document.get("projectUuid").and_then(Value::as_str).is_none_or(str::is_empty)
        || page_uuid.as_deref().is_none_or(str::is_empty)
    {
        inconclusive.insert(observation_face::DOCUMENT.to_owned());
    }

    let diagnostics = component_diagnostics(payload);
    for diagnostic in &diagnostics {
        if let Some(face) = diagnostic["face"].as_str() {
            inconclusive.insert(face.to_owned());
        }
    }
    let components = parse_components(payload, &mut inconclusive);
    let wires = parse_wires(payload, &mut inconclusive);
    if payload.get("buses").and_then(Value::as_array).is_none() {
        inconclusive.insert(observation_face::BUSES.to_owned());
    }
    let bus_names: Vec<String> = payload
        .get("buses")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|bus| bus.get("name").and_then(Value::as_str).map(str::to_owned))
        .collect();

    // 网络名：nets[] 缺失时从 wires[].net 归集，并记入 inconclusive；
    // nets[] 存在时仍与导线网络取并集——未同步进列表的新网络不静默丢弃。
    let mut net_names: BTreeSet<String> = BTreeSet::new();
    if let Some(items) = payload.get("nets").and_then(Value::as_array) {
        if items.iter().any(|net| net.get("name").and_then(Value::as_str).is_none_or(str::is_empty))
        {
            inconclusive.insert(observation_face::NETS.to_owned());
        }
        net_names.extend(
            items
                .iter()
                .filter_map(|net| net.get("name").and_then(Value::as_str).map(str::to_owned)),
        );
    } else {
        inconclusive.insert(observation_face::NETS.to_owned());
    }
    for wire in &wires {
        if let Some(net) = &wire.net {
            net_names.insert(net.clone());
        }
    }

    let membership = resolve_net_membership(&components, &wires, &mut inconclusive);
    let mut physical_projection =
        build_physical_projection(&document, page_uuid.as_deref(), &components, &wires, &bus_names);
    if let Some(symbols) = payload["symbols"].as_array().filter(|items| !items.is_empty()) {
        // 辅助符号不伪装成逻辑元件，但它们的名称、位置与引脚变化仍使审批基线失效。
        let mut projection: Vec<Value> = symbols
            .iter()
            .map(|symbol| {
                let mut pins = symbol["pins"].as_array().cloned().unwrap_or_default();
                pins.sort_by(|a, b| a["number"].as_str().cmp(&b["number"].as_str()));
                json!({"primitiveId":symbol["primitiveId"], "componentType":symbol["componentType"],
                "name":symbol["name"], "x":symbol["x"], "y":symbol["y"],
                "rotation":symbol["rotation"], "mirror":symbol["mirror"], "pins":pins})
            })
            .collect();
        projection.sort_by(|a, b| a["primitiveId"].as_str().cmp(&b["primitiveId"].as_str()));
        physical_projection["symbols"] = json!(projection);
    }
    let physical_hash =
        sha256_digest(&serde_json::to_vec(&physical_projection).expect("物理投影序列化不应失败"));
    let external_refs = build_external_refs(&components, &wires, &net_names, page_uuid.as_deref());

    let mut snapshot = LogicalCircuitSnapshot::empty(SnapshotAuthority::Observed);
    snapshot.parent_snapshot_hash = parent_snapshot_hash.map(str::to_owned);
    for component in &components {
        let mut pins: Vec<Pin> = component
            .pins
            .iter()
            .map(|pin| Pin { id: pin.number.clone(), name: pin.name.clone() })
            .collect();
        pins.sort_by(|a, b| a.id.cmp(&b.id));
        snapshot.components.push(Component {
            id: format!("component:{}", component.designator),
            reference: component.designator.clone(),
            value: component.value.clone(),
            pins,
            evidence: Vec::new(),
        });
    }
    for name in &net_names {
        let mut pins: Vec<PinRef> = membership
            .get(name)
            .into_iter()
            .flatten()
            .map(|(component_id, pin_id)| PinRef {
                component_id: component_id.clone(),
                pin_id: pin_id.clone(),
            })
            .collect();
        // 成员按 `元件:引脚` 字典序排序，保证确定性。
        pins.sort_by(|a, b| {
            format!("{}:{}", a.component_id, a.pin_id)
                .cmp(&format!("{}:{}", b.component_id, b.pin_id))
        });
        snapshot.nets.push(Net { id: format!("net:{name}"), name: Some(name.clone()), pins });
    }
    snapshot.physical_hash = Some(physical_hash.clone());
    let snapshot = circuitfabric_core::stamp_snapshot(&snapshot)?;

    let mut validation = circuitfabric_core::validate_snapshot(&snapshot);
    let inconclusive: Vec<String> = inconclusive.into_iter().collect();
    if !inconclusive.is_empty() {
        validation.push(ConstraintResult {
            constraint_id: "observation.completeness".to_owned(),
            layer: "observation".to_owned(),
            subject_refs: inconclusive.clone(),
            status: FactStatus::Inconclusive,
            severity: Severity::Warning,
            evidence_refs: Vec::new(),
            snapshot_hash: snapshot.snapshot_hash.clone(),
            explanation: "部分观测面缺失或不可用，本次观测不应视为完整。".to_owned(),
            recommendation: Some(if diagnostics.iter().any(|entry| entry["code"] == "missing_designator") {
                "先按官方器件类型区分辅助符号，或补齐真正缺位号的器件；更新扩展后重新采集并生成预览。反复读取已保存的快照不会补齐缺失字段。".to_owned()
            } else {
                "检查 diagnostics 中的具体字段与 API 问题，修复后在 EDA 扩展中重新采集；仅重读缓存不会产生新观测。".to_owned()
            }),
        });
    }

    Ok(ObservationBuild {
        snapshot,
        physical_hash,
        physical_projection,
        external_refs,
        inconclusive,
        validation,
        diagnostics,
    })
}

fn is_schematic_symbol(component_type: &str) -> bool {
    matches!(
        component_type,
        "sheet"
            | "netflag"
            | "netport"
            | "diffPairsFlag"
            | "nonElectrical_symbol"
            | "offPageConnector"
            | "short_symbol"
    )
}

/// 从原始观测解释元件采集失败；兼容没有 diagnostics 字段的旧记录。
/// 不以元件 evidence 为空推断故障，也不修改观测的完整性事实。
#[must_use]
pub fn component_diagnostics(payload: &Value) -> Vec<Value> {
    let mut groups: BTreeMap<(&str, &str, &str), (usize, BTreeSet<String>)> = BTreeMap::new();
    let mut note = |face, code, message, item: &Value| {
        let entry = groups.entry((face, code, message)).or_default();
        entry.0 += 1;
        if entry.1.len() < 5 {
            if let Some(name) = item["designator"]
                .as_str()
                .filter(|s| !s.is_empty())
                .or_else(|| item["name"].as_str())
            {
                entry.1.insert(name.to_owned());
            }
        }
    };
    for collection in ["components", "symbols"] {
        let Some(items) = payload[collection].as_array() else {
            if collection == "components" || !payload[collection].is_null() {
                note(
                    "components",
                    "collection_unavailable",
                    "元件或辅助符号集合不是有效数组，采集未完成。",
                    &Value::Null,
                );
            }
            continue;
        };
        for item in items {
            let component_type = item["componentType"].as_str();
            if collection == "symbols" && !component_type.is_some_and(is_schematic_symbol)
                || collection == "components" && component_type.is_some_and(|kind| kind != "part")
            {
                note(
                    "components",
                    "unsupported_component_type",
                    "对象类型与采集集合不匹配或尚不支持，不能当作普通器件放行。",
                    item,
                );
            }
            if collection == "components"
                && item["designator"].as_str().is_none_or(|s| s.trim().is_empty())
            {
                note(
                    "components",
                    "missing_designator",
                    "对象缺少位号；图框、电源标识和网络端口应按官方类型分类，真正缺位号的器件需补齐。",
                    item,
                );
            }
            if item["primitiveId"].as_str().is_none_or(|s| s.trim().is_empty()) {
                note(
                    "components",
                    "missing_primitive_id",
                    "对象缺少原始图元身份，无法可靠定位。",
                    item,
                );
            }
            if item["name"].as_str().is_none() {
                note("components", "missing_name", "对象名称读取失败或返回了无效类型。", item);
            }
            if ["x", "y", "rotation"].iter().any(|field| item[*field].as_f64().is_none())
                || item["mirror"].as_bool().is_none()
            {
                note(
                    "physicalPlacement",
                    "missing_placement",
                    "对象的位置、旋转或镜像数据不完整。",
                    item,
                );
            }
            if item["pins"].as_array().is_none_or(|pins| {
                pins.iter().any(|pin| {
                    pin["number"].as_str().is_none_or(str::is_empty)
                        || pin["x"].as_f64().is_none()
                        || pin["y"].as_f64().is_none()
                })
            }) {
                note("pins", "missing_pin_data", "对象的引脚列表、引脚号或坐标不完整。", item);
            }
        }
    }
    let mut diagnostics: Vec<Value> = groups.into_iter().map(|((face, code, message), (count, examples))|
        json!({"face":face,"code":code,"message":message,"count":count,"examples":examples})
    ).collect();
    if !diagnostics.iter().any(|item| item["face"] == "components")
        && payload["inconclusive"]
            .as_array()
            .is_some_and(|faces| faces.iter().any(|face| face == "components"))
    {
        diagnostics.push(json!({"face":"components","code":"executor_incomplete","count":1,
            "message":"执行器报告元件采集不完整；需检查扩展 API 调用或更新扩展，不能仅靠重读缓存恢复。","examples":[]}));
    }
    diagnostics
}

fn parse_components(
    payload: &Value,
    inconclusive: &mut BTreeSet<String>,
) -> Vec<ObservedComponent> {
    let mut components = Vec::new();
    let Some(items) = payload.get("components").and_then(Value::as_array) else {
        inconclusive.insert(observation_face::COMPONENTS.to_owned());
        inconclusive.insert(observation_face::PINS.to_owned());
        return components;
    };
    for item in items {
        let Some(designator) =
            item.get("designator").and_then(Value::as_str).filter(|value| !value.trim().is_empty())
        else {
            // 无位号的元件不入快照，观测面记为缺失。
            inconclusive.insert(observation_face::COMPONENTS.to_owned());
            continue;
        };
        let pins: Vec<ObservedPin> = if let Some(list) = item.get("pins").and_then(Value::as_array)
        {
            list.iter()
                .filter_map(|pin| {
                    let Some(number) =
                        pin.get("number").and_then(Value::as_str).filter(|s| !s.is_empty())
                    else {
                        inconclusive.insert(observation_face::PINS.to_owned());
                        return None;
                    };
                    if pin.get("x").and_then(Value::as_f64).is_none()
                        || pin.get("y").and_then(Value::as_f64).is_none()
                    {
                        inconclusive.insert(observation_face::PINS.to_owned());
                    }
                    Some(ObservedPin {
                        number: number.to_owned(),
                        name: pin
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                        x: pin.get("x").and_then(Value::as_f64),
                        y: pin.get("y").and_then(Value::as_f64),
                    })
                })
                .collect()
        } else {
            inconclusive.insert(observation_face::PINS.to_owned());
            Vec::new()
        };
        let x = item.get("x").and_then(Value::as_f64);
        let y = item.get("y").and_then(Value::as_f64);
        if x.is_none()
            || y.is_none()
            || item.get("rotation").and_then(Value::as_f64).is_none()
            || item.get("mirror").and_then(Value::as_bool).is_none()
        {
            inconclusive.insert(observation_face::PHYSICAL_PLACEMENT.to_owned());
        }
        if item.get("primitiveId").and_then(Value::as_str).is_none_or(str::is_empty)
            || item.get("name").and_then(Value::as_str).is_none()
        {
            inconclusive.insert(observation_face::COMPONENTS.to_owned());
        }
        components.push(ObservedComponent {
            designator: designator.to_owned(),
            value: item.get("name").and_then(Value::as_str).map(str::to_owned),
            primitive_id: item.get("primitiveId").and_then(Value::as_str).map(str::to_owned),
            footprint: item.get("footprint").and_then(Value::as_str).map(str::to_owned),
            x,
            y,
            rotation: item.get("rotation").and_then(Value::as_f64),
            mirror: item.get("mirror").and_then(Value::as_bool).unwrap_or(false),
            page_uuid: item
                .get("pageUuid")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| page_uuid_of(item)),
            pins,
        });
    }
    components.sort_by(|a, b| a.designator.cmp(&b.designator));
    components
}

fn page_uuid_of(item: &Value) -> Option<String> {
    item.get("page").and_then(|page| page.get("uuid")).and_then(Value::as_str).map(str::to_owned)
}

fn parse_wires(payload: &Value, inconclusive: &mut BTreeSet<String>) -> Vec<ObservedWire> {
    let Some(items) = payload.get("wires").and_then(Value::as_array) else {
        inconclusive.insert(observation_face::WIRES.to_owned());
        return Vec::new();
    };
    let mut wires = Vec::new();
    for item in items {
        let points: Vec<[f64; 2]> = item
            .get("points")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(|point| {
                        let pair = point.as_array()?;
                        Some([
                            pair.first().and_then(Value::as_f64)?,
                            pair.get(1).and_then(Value::as_f64)?,
                        ])
                    })
                    .collect()
            })
            .unwrap_or_default();
        if !item.get("points").and_then(Value::as_array).is_some_and(|raw| {
            raw.len() == points.len()
                && raw.iter().all(|point| point.as_array().is_some_and(|pair| pair.len() == 2))
        }) {
            inconclusive.insert(observation_face::WIRES.to_owned());
            inconclusive.insert(observation_face::NET_MEMBERSHIP.to_owned());
        }
        if points.len() < 2
            || item.get("primitiveId").and_then(Value::as_str).is_none_or(str::is_empty)
            || item.get("net").and_then(Value::as_str).is_none()
        {
            inconclusive.insert(observation_face::WIRES.to_owned());
            inconclusive.insert(observation_face::NET_MEMBERSHIP.to_owned());
        }
        wires.push(ObservedWire {
            primitive_id: item.get("primitiveId").and_then(Value::as_str).map(str::to_owned),
            net: item.get("net").and_then(Value::as_str).map(str::to_owned),
            points,
        });
    }
    wires.sort_by(|a, b| a.net.cmp(&b.net).then_with(|| a.primitive_id.cmp(&b.primitive_id)));
    wires
}

/// 坐标端点启发式：引脚坐标与某导线折线端点精确相等则属于该导线网络；
/// 一对多或无匹配均不列入成员，一对多时把 `netMembership` 记入 inconclusive。
fn resolve_net_membership(
    components: &[ObservedComponent],
    wires: &[ObservedWire],
    inconclusive: &mut BTreeSet<String>,
) -> BTreeMap<String, Vec<(String, String)>> {
    let mut endpoints: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for wire in wires {
        let Some(net) = wire.net.clone() else { continue };
        let ends = [wire.points.first(), wire.points.last()].into_iter().flatten();
        for point in ends {
            endpoints.entry(coordinate_key(*point)).or_default().insert(net.clone());
        }
    }
    let mut membership: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for component in components {
        for pin in &component.pins {
            let (Some(x), Some(y)) = (pin.x, pin.y) else { continue };
            let Some(nets) = endpoints.get(&coordinate_key([x, y])) else { continue };
            if nets.len() > 1 {
                inconclusive.insert(observation_face::NET_MEMBERSHIP.to_owned());
                continue;
            }
            let net = nets.iter().next().expect("非空集合").clone();
            membership
                .entry(net)
                .or_default()
                .push((format!("component:{}", component.designator), pin.number.clone()));
        }
    }
    membership
}

fn coordinate_key(point: [f64; 2]) -> String {
    format!("{:?}:{:?}", point[0], point[1])
}

/// 物理观测投影：位号、坐标、旋转、镜像、封装、页 UUID、导线几何，排序后摘要。
fn build_physical_projection(
    document: &Value,
    page_uuid: Option<&str>,
    components: &[ObservedComponent],
    wires: &[ObservedWire],
    bus_names: &[String],
) -> Value {
    let mut buses: Vec<Value> = bus_names.iter().map(|name| json!({ "name": name })).collect();
    buses.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    json!({
        "document": {
            "kind": document.get("kind").and_then(Value::as_str).unwrap_or("schematic"),
            "projectUuid": document.get("projectUuid"),
            "pageUuid": page_uuid,
        },
        "components": components.iter().map(|component| {
            // 引脚顺序也是 EDA 返回顺序的任意选择：按引脚号排序后投影。
            let mut pins: Vec<&ObservedPin> = component.pins.iter().collect();
            pins.sort_by(|a, b| a.number.cmp(&b.number));
            json!({
                "designator": component.designator,
                "x": component.x,
                "y": component.y,
                "rotation": component.rotation,
                "mirror": component.mirror,
                "footprint": component.footprint,
                "pageUuid": component.page_uuid,
                "pins": pins.iter().map(|pin| pin.projection_json()).collect::<Vec<_>>(),
            })
        }).collect::<Vec<_>>(),
        "wires": wires.iter().map(|wire| {
            // 只归一正反方向，保留折线连接次序；点集相同不代表路径相同。
            let mut points = wire.points.clone();
            let reverse: Vec<_> = points.iter().rev().copied().collect();
            if reverse < points { points = reverse; }
            json!({
                "primitiveId": wire.primitive_id,
                "net": wire.net,
                "points": points,
            })
        }).collect::<Vec<_>>(),
        "buses": buses,
    })
}

fn build_external_refs(
    components: &[ObservedComponent],
    wires: &[ObservedWire],
    net_names: &BTreeSet<String>,
    document_page_uuid: Option<&str>,
) -> Value {
    let mut refs: Vec<Value> = components
        .iter()
        .map(|component| {
            json!({
                "logicalId": format!("component:{}", component.designator),
                "primitiveId": component.primitive_id,
                "pageUuid": component.page_uuid.as_deref().or(document_page_uuid),
            })
        })
        .collect();
    for name in net_names {
        let wire_ids: Vec<Value> = wires
            .iter()
            .filter(|wire| wire.net.as_deref() == Some(name.as_str()))
            .filter_map(|wire| wire.primitive_id.clone().map(Value::String))
            .collect();
        refs.push(json!({
            "logicalId": format!("net:{name}"),
            "primitiveIds": wire_ids,
            "pageUuid": document_page_uuid,
        }));
    }
    json!(refs)
}

/// 观测记录的目标目录 `<项目根>/observations/`。
#[must_use]
pub fn observations_dir(project_root: &Path) -> PathBuf {
    project_root.join(OBSERVATIONS_DIRECTORY)
}

pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), ObservationError> {
    let dir = path.parent().expect("文件名前必有目录");
    fs::create_dir_all(dir)
        .map_err(|source| ObservationError::CreateDirectory { path: dir.to_path_buf(), source })?;
    let staging = path.with_extension("tmp");
    fs::write(&staging, bytes)
        .map_err(|source| ObservationError::Write { path: staging.clone(), source })?;
    fs::rename(&staging, path).map_err(|source| ObservationError::Commit {
        from: staging,
        to: path.to_path_buf(),
        source,
    })
}

/// 把 inspect 观测记录原子写入 `<项目根>/observations/<snapshotHash>.json`
/// （`sha256:` 前缀去除），返回写入路径。
///
/// # Errors
///
/// 返回 [`ObservationError`] 当目录创建、写入或提交失败，或记录缺少
/// `snapshot.snapshot_hash` 字段。
///
/// # Panics
///
/// 观测记录序列化为 JSON 不应失败；失败即为编程错误，直接 panic。
pub fn persist_inspect_record(
    project_root: &Path,
    record: &Value,
) -> Result<PathBuf, ObservationError> {
    let hash =
        record.pointer("/snapshot/snapshot_hash").and_then(Value::as_str).ok_or_else(|| {
            ObservationError::Write {
                path: observations_dir(project_root),
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "inspect 记录缺少 snapshot.snapshot_hash",
                ),
            }
        })?;
    let file_name = format!("{}.json", hash.strip_prefix("sha256:").unwrap_or(hash));
    let path = observations_dir(project_root).join(file_name);
    let bytes = serde_json::to_vec_pretty(record).expect("观测记录序列化不应失败");
    // 内容地址可以重复；外部图元映射却可能变化，保留每次观测才能识别位号复用。
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let history_path = observations_dir(project_root)
        .join("inspect-history")
        .join(format!("{timestamp:039}-{}-{sequence:020}.json", std::process::id()));
    write_atomic(&history_path, &bytes)?;
    write_atomic(&path, &bytes)?;
    Ok(path)
}

/// 读取同项目最近一次 inspect 记录的 `snapshotHash`（`parent_snapshot_hash` 链）。
///
/// # Errors
///
/// 返回 [`ObservationError`] 当观测目录可读但记录解析失败；目录不存在时返回 `Ok(None)`。
pub fn latest_inspect_snapshot_hash(
    project_root: &Path,
) -> Result<Option<String>, ObservationError> {
    let history = observations_dir(project_root).join("inspect-history");
    let dir = if history.is_dir() { history } else { observations_dir(project_root) };
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(ObservationError::Read { path: dir, source }),
    };
    let mut latest: Option<((String, String), String)> = None;
    for entry in entries {
        let entry = entry.map_err(|source| ObservationError::Read { path: dir.clone(), source })?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let raw = fs::read_to_string(&path)
            .map_err(|source| ObservationError::Read { path: path.clone(), source })?;
        let Ok(record) = serde_json::from_str::<Value>(&raw) else { continue };
        if record.get("kind").and_then(Value::as_str) != Some("inspect") {
            continue;
        }
        let Some(hash) = record.pointer("/snapshot/snapshot_hash").and_then(Value::as_str) else {
            continue;
        };
        let received =
            record.get("receivedAt").and_then(Value::as_str).unwrap_or_default().to_owned();
        // 同毫秒观测按持久化顺序决胜，与融合服务使用同一排序规则。
        let order = (received, path.file_name().unwrap_or_default().to_string_lossy().into_owned());
        if latest.as_ref().is_none_or(|(when, _)| order > *when) {
            latest = Some((order, hash.to_owned()));
        }
    }
    Ok(latest.map(|(_, hash)| hash))
}

/// 把 DRC 报告写入 `<项目根>/observations/drc-<unix秒>.json`，返回写入路径。
///
/// # Errors
///
/// 返回 [`ObservationError`] 当目录创建、写入或提交失败。
///
/// # Panics
///
/// DRC 报告序列化为 JSON 不应失败；失败即为编程错误，直接 panic。
pub fn persist_drc_report(
    project_root: &Path,
    report: &Value,
) -> Result<PathBuf, ObservationError> {
    let seconds =
        report.get("receivedAtUnixSeconds").and_then(Value::as_u64).unwrap_or_else(unix_seconds);
    let path = observations_dir(project_root).join(format!("drc-{seconds}.json"));
    let bytes = serde_json::to_vec_pretty(report).expect("DRC 报告序列化不应失败");
    write_atomic(&path, &bytes)?;
    Ok(path)
}

/// 把视觉证据 PNG 字节与同名 JSON 元数据写入 `<项目根>/observations/capture-<unix秒>.*`，
/// 返回 PNG 路径与内容摘要。
///
/// # Errors
///
/// 返回 [`ObservationError`] 当目录创建、写入或提交失败。
///
/// # Panics
///
/// 元数据序列化为 JSON 不应失败；失败即为编程错误，直接 panic。
pub fn persist_visual_capture(
    project_root: &Path,
    png: &[u8],
    metadata: &Value,
) -> Result<(PathBuf, String), ObservationError> {
    let seconds =
        metadata.get("receivedAtUnixSeconds").and_then(Value::as_u64).unwrap_or_else(unix_seconds);
    let content_hash = sha256_digest(png);
    let png_path = observations_dir(project_root).join(format!("capture-{seconds}.png"));
    write_atomic(&png_path, png)?;
    let meta_path = png_path.with_extension("json");
    write_atomic(&meta_path, &serde_json::to_vec_pretty(metadata).expect("元数据序列化不应失败"))?;
    Ok((png_path, content_hash))
}

fn unix_seconds() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()
}

#[cfg(test)]
mod tests {
    use circuitfabric_contracts::FactStatus;

    use super::*;

    fn fixture_payload() -> Value {
        json!({
            "document": { "kind": "schematic", "projectUuid": "p1", "projectName": "电源",
                          "pageUuid": "page-1", "pageName": "Sheet1" },
            "components": [
                { "primitiveId": "prim-r1", "designator": "R1", "name": "10k", "footprint": "0603",
                  "x": 0, "y": 0, "rotation": 90, "mirror": false,
                  "pins": [ { "number": "1", "name": "A", "x": 0, "y": 0 },
                            { "number": "2", "name": "B", "x": 0, "y": 10 } ] },
                { "primitiveId": "prim-r2", "designator": "R2", "name": "1k", "footprint": "0603",
                  "x": 40, "y": 0, "rotation": 0, "mirror": false,
                  "pins": [ { "number": "1", "name": "A", "x": 40, "y": 10 },
                            { "number": "2", "name": "B", "x": 40, "y": 0 } ] }
            ],
            "wires": [
                { "primitiveId": "w1", "net": "VCC", "points": [[0, 0], [40, 0]] }
            ],
            "buses": [ { "primitiveId": "b1", "name": "DBUS" } ],
            "nets": [ { "name": "VCC", "wires": [ { "name": "w1", "id": "w1", "pageName": "Sheet1", "pageUuid": "page-1" } ] } ]
        })
    }

    #[test]
    fn observation_building_is_stable() {
        let first = build_observation(&fixture_payload(), None).expect("构建");
        let second = build_observation(&fixture_payload(), None).expect("构建");
        assert_eq!(first.snapshot.snapshot_hash, second.snapshot.snapshot_hash);
        assert_eq!(first.snapshot.logical_hash, second.snapshot.logical_hash);
        assert_eq!(first.physical_hash, second.physical_hash);
        assert!(first.inconclusive.is_empty());
        assert!(first.validation.iter().all(|result| result.status == FactStatus::Passed));
    }

    #[test]
    fn input_order_does_not_change_hashes() {
        let shuffled = json!({
            "nets": [ { "name": "VCC", "wires": [] } ],
            "wires": [ { "primitiveId": "w1", "net": "VCC", "points": [[40, 0], [0, 0]] } ],
            "buses": [ { "name": "DBUS" } ],
            "document": { "kind": "schematic", "projectUuid": "p1", "pageUuid": "page-1" },
            "components": [
                { "primitiveId": "prim-r1", "designator": "R1", "name": "10k", "footprint": "0603",
                  "x": 0, "y": 0, "rotation": 90, "mirror": false,
                  "pins": [ { "number": "2", "name": "B", "x": 0, "y": 10 },
                            { "number": "1", "name": "A", "x": 0, "y": 0 } ] },
                { "primitiveId": "prim-r2", "designator": "R2", "name": "1k", "footprint": "0603",
                  "x": 40, "y": 0, "rotation": 0, "mirror": false,
                  "pins": [ { "number": "1", "name": "A", "x": 40, "y": 10 },
                            { "number": "2", "name": "B", "x": 40, "y": 0 } ] }
            ]
        });
        let ordered = build_observation(&fixture_payload(), None).expect("构建");
        let shuffled_build = build_observation(&shuffled, None).expect("构建");
        assert_eq!(ordered.snapshot.snapshot_hash, shuffled_build.snapshot.snapshot_hash);
        assert_eq!(ordered.physical_hash, shuffled_build.physical_hash);
    }

    #[test]
    fn physical_only_change_keeps_logical_hash() {
        let mut moved = fixture_payload();
        moved["components"][1]["x"] = json!(5);
        let baseline = build_observation(&fixture_payload(), None).expect("构建");
        let moved_build = build_observation(&moved, None).expect("构建");
        assert_eq!(baseline.snapshot.logical_hash, moved_build.snapshot.logical_hash);
        assert_ne!(baseline.physical_hash, moved_build.physical_hash);
        assert_ne!(baseline.snapshot.snapshot_hash, moved_build.snapshot.snapshot_hash);
    }

    #[test]
    fn polyline_topology_and_missing_fields_remain_observable() {
        let mut first = fixture_payload();
        first["wires"][0]["points"] = json!([[0, 0], [0, 10], [40, 10], [40, 0]]);
        let mut second = first.clone();
        second["wires"][0]["points"] = json!([[0, 0], [40, 10], [0, 10], [40, 0]]);
        assert_ne!(
            build_observation(&first, None).unwrap().physical_hash,
            build_observation(&second, None).unwrap().physical_hash
        );
        first["components"][0]["pins"][0]["number"] = Value::Null;
        first["components"][0]["rotation"] = Value::Null;
        first["document"]["pageUuid"] = Value::Null;
        first["inconclusive"] = json!(["nets"]);
        let build = build_observation(&first, None).unwrap();
        for face in ["pins", "physicalPlacement", "document", "nets"] {
            assert!(build.inconclusive.iter().any(|item| item == face));
        }
    }

    #[test]
    fn missing_nets_are_inconclusive_and_derived_from_wires() {
        let mut payload = fixture_payload();
        payload.as_object_mut().expect("对象").remove("nets");
        let build = build_observation(&payload, None).expect("构建");
        assert!(build.inconclusive.contains(&observation_face::NETS.to_owned()));
        assert!(build.validation.iter().any(
            |result| result.layer == "observation" && result.status == FactStatus::Inconclusive
        ));
        assert!(build.snapshot.nets.iter().any(|net| net.id == "net:VCC"));
    }

    #[test]
    fn duplicate_designators_fail_validation() {
        let mut payload = fixture_payload();
        let duplicate = payload["components"][0].clone();
        payload["components"].as_array_mut().expect("数组").push(duplicate);
        let build = build_observation(&payload, None).expect("构建");
        assert!(
            build
                .validation
                .iter()
                .any(|result| result.constraint_id == "identity.unique_component_id"
                    && result.status == FactStatus::Failed)
        );
    }

    #[test]
    fn pin_endpoint_membership_resolves_and_ambiguity_is_inconclusive() {
        let build = build_observation(&fixture_payload(), None).expect("构建");
        let vcc = build.snapshot.nets.iter().find(|net| net.id == "net:VCC").expect("VCC 网络");
        let pins: Vec<String> =
            vcc.pins.iter().map(|pin| format!("{}:{}", pin.component_id, pin.pin_id)).collect();
        assert_eq!(pins, vec!["component:R1:1", "component:R2:2"]);

        // R1 引脚 1（0,0）同时落在 VCC 与 GND 两条导线的端点上：一对多不列入成员且记 inconclusive。
        let mut ambiguous = fixture_payload();
        ambiguous["wires"].as_array_mut().expect("数组").push(json!({
            "primitiveId": "w2", "net": "GND", "points": [[0, 0], [10, 10]]
        }));
        let build = build_observation(&ambiguous, None).expect("构建");
        assert!(build.inconclusive.contains(&observation_face::NET_MEMBERSHIP.to_owned()));
        let gnd = build.snapshot.nets.iter().find(|net| net.id == "net:GND").expect("GND 网络");
        assert!(gnd.pins.is_empty());
    }

    #[test]
    fn missing_designator_drops_component_and_flags_components_face() {
        let mut payload = fixture_payload();
        payload["components"][0].as_object_mut().expect("对象").remove("designator");
        let build = build_observation(&payload, None).expect("构建");
        assert!(build.inconclusive.contains(&observation_face::COMPONENTS.to_owned()));
        assert_eq!(build.snapshot.components.len(), 1);
        assert_eq!(build.snapshot.components[0].id, "component:R2");
        let diagnostic =
            build.diagnostics.iter().find(|entry| entry["code"] == "missing_designator").unwrap();
        assert_eq!(diagnostic["count"], 1);
        assert!(diagnostic["message"].as_str().unwrap().contains("位号"));
    }

    #[test]
    fn classified_symbols_preserve_evidence_and_affect_physical_baselines() {
        let mut payload = fixture_payload();
        payload["symbols"] = json!([
            {"componentType":"sheet","primitiveId":"frame","name":"Drawing-Symbol_A4",
             "x":0,"y":0,"rotation":0,"mirror":false,"pins":[]},
            {"componentType":"netflag","primitiveId":"gnd","name":"GND",
             "x":10,"y":20,"rotation":0,"mirror":false,"pins":[]},
            {"componentType":"netport","primitiveId":"port","name":"EN",
             "x":50,"y":40,"rotation":0,"mirror":false,"pins":[]}
        ]);
        let first = build_observation(&payload, None).unwrap();
        assert!(first.inconclusive.is_empty());
        assert!(first.diagnostics.is_empty());
        assert_eq!(first.snapshot.components.len(), 2);
        assert_eq!(first.physical_projection["symbols"].as_array().unwrap().len(), 3);
        payload["symbols"].as_array_mut().unwrap().reverse();
        let reordered = build_observation(&payload, None).unwrap();
        assert_eq!(first.snapshot.snapshot_hash, reordered.snapshot.snapshot_hash);
        assert_eq!(first.external_refs, reordered.external_refs);
        payload["symbols"][0]["name"] = json!("OTHER_NET");
        let renamed = build_observation(&payload, None).unwrap();
        assert_eq!(first.snapshot.logical_hash, renamed.snapshot.logical_hash);
        assert_ne!(first.physical_hash, renamed.physical_hash);
        assert_ne!(first.snapshot.snapshot_hash, renamed.snapshot.snapshot_hash);
        // 不能把真正的普通器件塞进 symbols 来绕过位号与身份检查。
        payload["symbols"][0]["componentType"] = json!("part");
        let invalid = build_observation(&payload, None).unwrap();
        assert!(invalid.inconclusive.iter().any(|face| face == "components"));
    }

    #[test]
    fn component_diagnostics_are_specific_and_bounded_without_assuming_evidence() {
        let mut payload = fixture_payload();
        let mut bad = payload["components"][0].clone();
        bad["designator"] = Value::Null;
        bad["name"] = json!("GND");
        for _ in 0..45 {
            payload["components"].as_array_mut().unwrap().push(bad.clone());
        }
        let diagnostics = component_diagnostics(&payload);
        let missing =
            diagnostics.iter().find(|entry| entry["code"] == "missing_designator").unwrap();
        assert_eq!(missing["count"], 45);
        assert_eq!(missing["examples"], json!(["GND"]));
        assert!(!serde_json::to_string(&diagnostics).unwrap().contains("prim-r1"));
        let complete = build_observation(&fixture_payload(), None).unwrap();
        assert!(complete.snapshot.components.iter().all(|component| component.evidence.is_empty()));
        assert!(complete.inconclusive.is_empty(), "evidence 为空本身不表示采集不完整");
    }

    #[test]
    fn records_roundtrip_and_parent_chain_links() {
        let dir = std::env::temp_dir().join(format!(
            "circuitfabric-observation-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos()
        ));
        let root = dir.join("project");
        std::fs::create_dir_all(&root).expect("创建项目根");

        let first = build_observation(&fixture_payload(), None).expect("构建");
        let first_record = json!({
            "schemaVersion": 1,
            "kind": "inspect",
            "projectId": "power-supply",
            "sessionId": "s1",
            "document": fixture_payload()["document"],
            "capturedAt": "2026-10-22T08:00:00.000Z",
            "receivedAt": "2026-10-22T08:00:01.000Z",
            "snapshot": first.snapshot,
            "externalRefs": first.external_refs,
            "inconclusive": first.inconclusive,
            "validation": first.validation,
            "physicalProjection": first.physical_projection,
        });
        let path = persist_inspect_record(&root, &first_record).expect("落盘");
        assert!(path.is_file());
        assert_eq!(
            latest_inspect_snapshot_hash(&root).expect("读取"),
            Some(first.snapshot.snapshot_hash.clone())
        );

        // 同内容二次 inspect 产生相同哈希；parent 链指向第一次（同哈希）。
        let second = build_observation(
            &fixture_payload(),
            latest_inspect_snapshot_hash(&root).expect("读取").as_deref(),
        )
        .expect("构建");
        assert_eq!(second.snapshot.snapshot_hash, first.snapshot.snapshot_hash);
        assert_eq!(
            second.snapshot.parent_snapshot_hash.as_deref(),
            Some(first.snapshot.snapshot_hash.as_str())
        );

        // 物理变化 → 新哈希，parent 链为第一次哈希，两次记录并存。
        let mut moved = fixture_payload();
        moved["components"][1]["x"] = json!(5);
        let third = build_observation(
            &moved,
            latest_inspect_snapshot_hash(&root).expect("读取").as_deref(),
        )
        .expect("构建");
        let third_record = json!({
            "schemaVersion": 1,
            "kind": "inspect",
            "projectId": "power-supply",
            "sessionId": "s1",
            "capturedAt": "2026-10-22T08:01:00.000Z",
            "receivedAt": "2026-10-22T08:01:01.000Z",
            "snapshot": third.snapshot,
            "externalRefs": third.external_refs,
            "inconclusive": third.inconclusive,
            "validation": third.validation,
            "physicalProjection": third.physical_projection,
        });
        let third_path = persist_inspect_record(&root, &third_record).expect("落盘");
        assert_ne!(third_path, path);
        assert!(third_path.is_file());
        assert_eq!(
            latest_inspect_snapshot_hash(&root).expect("读取"),
            Some(third.snapshot.snapshot_hash.clone())
        );
        let parsed: Value =
            serde_json::from_slice(&std::fs::read(&third_path).expect("读取")).expect("解析");
        assert_eq!(parsed["kind"], "inspect");
        assert_eq!(parsed["snapshot"]["parent_snapshot_hash"], first.snapshot.snapshot_hash);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn drc_and_capture_persist_next_to_inspect_records() {
        let dir = std::env::temp_dir().join(format!(
            "circuitfabric-observation-evidence-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos()
        ));
        let root = dir.join("project");
        let report = json!({
            "schemaVersion": 1,
            "kind": "drc",
            "receivedAtUnixSeconds": 1_800_000_000_u64,
            "status": "passed",
            "errors": [],
            "inconclusive": [],
        });
        let drc_path = persist_drc_report(&root, &report).expect("落盘");
        assert_eq!(
            drc_path.file_name().and_then(|name| name.to_str()),
            Some("drc-1800000000.json")
        );

        let png = b"\x89PNG\r\n\x1a\nfixture";
        let metadata = json!({
            "schemaVersion": 1,
            "kind": "visual_capture",
            "receivedAtUnixSeconds": 1_800_000_000_u64,
            "contentHash": sha256_digest(png),
            "mime": "image/png",
            "bytes": png.len(),
        });
        let (png_path, hash) = persist_visual_capture(&root, png, &metadata).expect("落盘");
        assert_eq!(hash, sha256_digest(png));
        assert_eq!(std::fs::read(&png_path).expect("读取 PNG"), png);
        assert!(png_path.with_extension("json").is_file());

        std::fs::remove_dir_all(&dir).ok();
    }
}
