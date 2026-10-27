//! IR/身份融合：把历次 inspect 观测的外部引用映射（EDA 图元 ID ↔
//! CircuitFabric 逻辑 ID）融合成独立视图，处理漂移与冲突。
//!
//! 铁律：逻辑身份永远来自快照构建规则（`component:<designator>` /
//! `net:<name>`），EDA 图元 ID（primitiveId）只作外部引用，绝不成为逻辑身份。
//!
//! 融合视图持久化到 `<项目根>/observations/fusion.json`，每次成功 inspect
//! 后由 Bridge 原子重写。

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::observation::{observations_dir, rfc3339_millis, write_atomic};

/// 融合视图文件名（位于 `<项目根>/observations/`）。
pub const FUSION_FILE_NAME: &str = "fusion.json";

#[derive(Debug, Error)]
pub enum FusionError {
    #[error("观测记录读取失败（{path}）：{source}")]
    Read { path: PathBuf, source: std::io::Error },
    #[error("融合视图写入失败（{path}）：{source}")]
    Write { path: PathBuf, source: std::io::Error },
    #[error("融合视图提交失败（{from} -> {to}）：{source}")]
    Commit { from: PathBuf, to: PathBuf, source: std::io::Error },
    #[error("观测记录不是有效 JSON（{path}）：{source}")]
    Parse { path: PathBuf, source: serde_json::Error },
    #[error("基线快照解析失败（{path}）：{source}")]
    Snapshot { path: PathBuf, source: serde_json::Error },
}

/// 一次成功 inspect 里对融合有意义的部分。
#[derive(Clone, Debug)]
pub struct InspectObservation {
    /// 该次观测的 snapshotHash（`sha256:…`）。
    pub snapshot_hash: String,
    /// 观测记录落盘时间（RFC 3339，排序键）。
    pub received_at: String,
    /// 观测记录中的 `externalRefs`（逻辑 ID → 图元 ID + 页 UUID）。
    pub external_refs: Value,
}

/// 映射状态：stable（历次观测一致）| drifted（跨观测漂移）| conflicted（冲突）。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MappingStatus {
    Stable,
    Drifted,
    Conflicted,
}

impl MappingStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Drifted => "drifted",
            Self::Conflicted => "conflicted",
        }
    }

    /// 身份是否可用于写入物化（无漂移、无冲突）。
    #[must_use]
    pub const fn is_stable(self) -> bool {
        matches!(self, Self::Stable)
    }
}

/// 一个逻辑身份跨观测的外部引用融合结果。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityMapping {
    /// CircuitFabric 逻辑 ID（`component:<designator>` / `net:<name>`）。
    pub logical_id: String,
    /// 历次观测中映射到的全部图元 ID（排序去重；漂移时保留全部历史）。
    pub primitive_ids: Vec<String>,
    /// 最近一次观测中看到的页 UUID。
    pub page_uuid: Option<String>,
    /// 首次/最近一次出现该逻辑 ID 的观测快照哈希。
    pub first_seen: String,
    pub last_seen: String,
    pub status: MappingStatus,
    /// 最新一次观测中是否仍出现；为 false 时只代表历史知识，不代表当前存在。
    pub present_in_latest: bool,
}

/// 一次观测内检测到的身份冲突。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityConflict {
    /// 首次发现该冲突的观测快照哈希。
    pub snapshot_hash: String,
    /// 被多个逻辑 ID 引用的图元 ID；互斥映射（同逻辑 ID 同观测多映射）时为 `None`。
    pub primitive_id: Option<String>,
    /// 涉及的全部逻辑 ID（排序去重）。
    pub logical_ids: Vec<String>,
    /// 人类可读的冲突说明。
    pub description: String,
}

/// 一个项目的当前融合视图。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FusionView {
    pub mappings: Vec<IdentityMapping>,
    pub conflicts: Vec<IdentityConflict>,
}

impl FusionView {
    /// 取某逻辑 ID 的映射（无则 `None`）。
    #[must_use]
    pub fn mapping_for(&self, logical_id: &str) -> Option<&IdentityMapping> {
        self.mappings.iter().find(|mapping| mapping.logical_id == logical_id)
    }
}

/// 读取项目的全部 inspect 观测记录，按 `receivedAt` 升序（同刻按哈希，
/// 保证确定性）。目录不存在时返回空向量；单个记录 JSON 损坏时跳过
/// （与 `latest_inspect_snapshot_hash` 的容错语义一致）。
///
/// # Errors
///
/// 返回 [`FusionError::Read`] 当目录读取失败（不存在除外）。
pub fn load_inspect_observations(
    project_root: &Path,
) -> Result<Vec<InspectObservation>, FusionError> {
    let history = observations_dir(project_root).join("inspect-history");
    let dir = if history.is_dir() { history } else { observations_dir(project_root) };
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(FusionError::Read { path: dir, source }),
    };
    let mut observations = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| FusionError::Read { path: dir.clone(), source })?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let raw = std::fs::read_to_string(&path)
            .map_err(|source| FusionError::Read { path: path.clone(), source })?;
        let Ok(record) = serde_json::from_str::<Value>(&raw) else { continue };
        if record.get("kind").and_then(Value::as_str) != Some("inspect") {
            continue;
        }
        let Some(snapshot_hash) =
            record.pointer("/snapshot/snapshot_hash").and_then(Value::as_str).map(str::to_owned)
        else {
            continue;
        };
        let received_at = format!(
            "{}/{}",
            record.get("receivedAt").and_then(Value::as_str).unwrap_or_default(),
            path.file_name().unwrap_or_default().to_string_lossy()
        );
        observations.push(InspectObservation {
            snapshot_hash,
            received_at,
            external_refs: record.get("externalRefs").cloned().unwrap_or(Value::Null),
        });
    }
    observations.sort_by(|a, b| {
        a.received_at.cmp(&b.received_at).then_with(|| a.snapshot_hash.cmp(&b.snapshot_hash))
    });
    Ok(observations)
}

/// 从按序观测记录构建融合视图。
#[must_use]
pub fn build_fusion_view(observations: &[InspectObservation]) -> FusionView {
    /// 单个逻辑 ID 的跨观测累积。
    struct Accum {
        sets: Vec<BTreeSet<(Option<String>, String)>>,
        /// 与 `sets` 一一对应的观测序号。
        indices: Vec<usize>,
        page_uuid: Option<String>,
        first_seen: String,
        last_seen: String,
        last_index: usize,
    }

    let mut accums: BTreeMap<String, Accum> = BTreeMap::new();
    let mut conflicted_ids: BTreeSet<String> = BTreeSet::new();
    let mut drifted_ids: BTreeSet<String> = BTreeSet::new();
    // 逐观测记录的冲突/图元换主事件，供"最近一次变化以来"的窗口判定。
    let mut conflict_at: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
    let mut drift_at: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
    let mut historical_owners: BTreeMap<(Option<String>, String), BTreeSet<String>> =
        BTreeMap::new();
    let mut conflicts = Vec::new();

    for (observation_index, observation) in observations.iter().enumerate() {
        // 单次观测内：图元 ID → 引用它的逻辑 ID 集合（双归属检测），
        // 以及逻辑 ID 是否重复出现（同观测互斥映射检测）。
        let mut owners: BTreeMap<(Option<String>, String), BTreeSet<String>> = BTreeMap::new();
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut duplicates = BTreeSet::new();

        let entries = observation.external_refs.as_array().map(Vec::as_slice).unwrap_or(&[]);
        for entry in entries {
            let Some(logical_id) = entry.get("logicalId").and_then(Value::as_str) else {
                continue;
            };
            if !seen.insert(logical_id.to_owned()) {
                duplicates.insert(logical_id.to_owned());
            }
            let primitives = entry_primitives(entry);
            let page_uuid = entry.get("pageUuid").and_then(Value::as_str).map(str::to_owned);
            for primitive in &primitives {
                owners
                    .entry((page_uuid.clone(), primitive.clone()))
                    .or_default()
                    .insert(logical_id.to_owned());
            }
            let accum = accums.entry(logical_id.to_owned()).or_insert_with(|| Accum {
                sets: Vec::new(),
                indices: Vec::new(),
                page_uuid: None,
                first_seen: observation.snapshot_hash.clone(),
                last_seen: String::new(),
                last_index: observation_index,
            });
            accum.sets.push(primitives.into_iter().map(|id| (page_uuid.clone(), id)).collect());
            accum.indices.push(observation_index);
            accum.page_uuid = page_uuid;
            accum.last_seen = observation.snapshot_hash.clone();
            accum.last_index = observation_index;
        }

        for logical_id in duplicates {
            conflict_at.entry(logical_id.clone()).or_default().insert(observation_index);
            conflicts.push(IdentityConflict {
                snapshot_hash: observation.snapshot_hash.clone(),
                primitive_id: None,
                logical_ids: vec![logical_id.clone()],
                description: format!(
                    "逻辑 ID `{logical_id}` 在同一次观测中出现多条互斥的外部引用映射。"
                ),
            });
            conflicted_ids.insert(logical_id);
        }
        for (reference, logical_ids) in &owners {
            let previous = historical_owners.entry(reference.clone()).or_default();
            if !previous.is_empty() && previous != logical_ids {
                drifted_ids.extend(previous.iter().cloned());
                drifted_ids.extend(logical_ids.iter().cloned());
                for id in previous.iter().chain(logical_ids) {
                    drift_at.entry(id.clone()).or_default().insert(observation_index);
                }
            }
            previous.extend(logical_ids.iter().cloned());
            if logical_ids.len() < 2 {
                continue;
            }
            let primitive_id = &reference.1;
            let ids: Vec<String> = logical_ids.iter().cloned().collect();
            for id in &ids {
                conflict_at.entry(id.clone()).or_default().insert(observation_index);
            }
            conflicts.push(IdentityConflict {
                snapshot_hash: observation.snapshot_hash.clone(),
                primitive_id: Some(primitive_id.clone()),
                logical_ids: ids.clone(),
                description: format!(
                    "图元 `{primitive_id}` 在同一次观测中被多个逻辑 ID 引用：{}。",
                    ids.join(", ")
                ),
            });
            conflicted_ids.extend(ids);
        }
    }

    let mappings = accums
        .into_iter()
        .map(|(logical_id, accum)| {
            let union: BTreeSet<String> =
                accum.sets.iter().flatten().map(|(_, id)| id.clone()).collect();
            let present_in_latest = Some(accum.last_index) == observations.len().checked_sub(1);
            if present_in_latest {
                // 当前存在的对象只看"最近一次变化以来"的连续观测：身份刚变化时
                // 标为漂移，再观测一次仍一致即确认稳定；更早的历史保留但不永久阻塞。
                let latest = accum.sets.last().cloned().unwrap_or_default();
                let mut run = 1;
                while run < accum.sets.len() {
                    let k = accum.sets.len() - 1 - run;
                    if accum.sets[k] != latest || accum.indices[k] + 1 != accum.indices[k + 1] {
                        break;
                    }
                    run += 1;
                }
                let run_start = accum.indices[accum.indices.len() - run];
                let had_change = run < accum.sets.len()
                    || drift_at.get(&logical_id).is_some_and(|at| at.contains(&run_start));
                let conflicted =
                    conflict_at.get(&logical_id).is_some_and(|at| at.contains(&accum.last_index));
                let status = if conflicted || latest.is_empty() {
                    MappingStatus::Conflicted
                } else if run == 1 && had_change {
                    MappingStatus::Drifted
                } else {
                    MappingStatus::Stable
                };
                let primitive_ids: BTreeSet<String> = if status == MappingStatus::Stable {
                    latest.iter().map(|(_, id)| id.clone()).collect()
                } else {
                    union
                };
                return IdentityMapping {
                    logical_id,
                    primitive_ids: primitive_ids.into_iter().collect(),
                    page_uuid: accum.page_uuid,
                    first_seen: accum.first_seen,
                    last_seen: accum.last_seen,
                    status,
                    present_in_latest,
                };
            }
            let status = if conflicted_ids.contains(&logical_id) {
                MappingStatus::Conflicted
            } else {
                let distinct: BTreeSet<_> = accum.sets.iter().collect();
                if distinct.len() > 1 || drifted_ids.contains(&logical_id) {
                    MappingStatus::Drifted
                } else if union.is_empty() {
                    MappingStatus::Conflicted
                } else {
                    MappingStatus::Stable
                }
            };
            IdentityMapping {
                logical_id,
                primitive_ids: union.into_iter().collect(),
                page_uuid: accum.page_uuid,
                first_seen: accum.first_seen,
                last_seen: accum.last_seen,
                status,
                present_in_latest,
            }
        })
        .collect();

    FusionView { mappings, conflicts }
}

/// 归一化一条 externalRefs 条目：元件用 `primitiveId`，网络用 `primitiveIds`。
fn entry_primitives(entry: &Value) -> Vec<String> {
    if let Some(primitive) =
        entry.get("primitiveId").and_then(Value::as_str).filter(|id| !id.trim().is_empty())
    {
        return vec![primitive.to_owned()];
    }
    entry
        .get("primitiveIds")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .filter(|id| !id.trim().is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// 重建并把融合视图原子写入 `<项目根>/observations/fusion.json`，返回路径与视图。
///
/// # Errors
///
/// 返回 [`FusionError`] 当观测读取或视图写入失败。
pub fn rebuild_and_persist(project_root: &Path) -> Result<(PathBuf, FusionView), FusionError> {
    let observations = load_inspect_observations(project_root)?;
    let view = build_fusion_view(&observations);
    let path = persist_fusion_view(project_root, &view)?;
    Ok((path, view))
}

/// 把融合视图原子写入 `<项目根>/observations/fusion.json`，返回写入路径。
///
/// # Errors
///
/// 返回 [`FusionError`] 当目录创建、写入或提交失败。
pub fn persist_fusion_view(project_root: &Path, view: &FusionView) -> Result<PathBuf, FusionError> {
    let path = observations_dir(project_root).join(FUSION_FILE_NAME);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let document = serde_json::json!({
        "schemaVersion": 1,
        "generatedAt": rfc3339_millis(now),
        "mappings": view.mappings,
        "conflicts": view.conflicts,
    });
    let bytes = serde_json::to_vec_pretty(&document).expect("融合视图序列化不应失败");
    write_atomic(&path, &bytes).map_err(|error| match error {
        crate::observation::ObservationError::CreateDirectory { path, source }
        | crate::observation::ObservationError::Write { path, source } => {
            FusionError::Write { path, source }
        }
        crate::observation::ObservationError::Commit { from, to, source } => {
            FusionError::Commit { from, to, source }
        }
        other => FusionError::Write {
            path: path.clone(),
            source: std::io::Error::new(std::io::ErrorKind::Other, other.to_string()),
        },
    })?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use circuitfabric_contracts::LogicalCircuitSnapshot;
    use serde_json::json;

    use super::*;

    fn observation(hash: &str, received: &str, refs: Value) -> InspectObservation {
        InspectObservation {
            snapshot_hash: hash.to_owned(),
            received_at: received.to_owned(),
            external_refs: refs,
        }
    }

    fn component_ref(logical: &str, primitive: &str, page: &str) -> Value {
        json!({ "logicalId": logical, "primitiveId": primitive, "pageUuid": page })
    }

    fn net_ref(logical: &str, primitives: &[&str]) -> Value {
        json!({ "logicalId": logical, "primitiveIds": primitives })
    }

    #[test]
    fn stable_mappings_stay_stable_across_observations() {
        let observations = vec![
            observation(
                "sha256:a",
                "2026-10-22T08:00:00.000Z",
                json!([
                    component_ref("component:R1", "prim-r1", "page-1"),
                    net_ref("net:VCC", &["w1"]),
                ]),
            ),
            observation(
                "sha256:b",
                "2026-10-22T08:01:00.000Z",
                json!([
                    component_ref("component:R1", "prim-r1", "page-1"),
                    net_ref("net:VCC", &["w1"]),
                ]),
            ),
        ];
        let view = build_fusion_view(&observations);

        assert!(view.conflicts.is_empty());
        assert_eq!(view.mappings.len(), 2);
        for mapping in &view.mappings {
            assert_eq!(mapping.status, MappingStatus::Stable, "{mapping:?}");
            assert!(mapping.present_in_latest);
        }
        let r1 = view.mapping_for("component:R1").expect("R1 映射");
        assert_eq!(r1.primitive_ids, vec!["prim-r1"]);
        assert_eq!(r1.first_seen, "sha256:a");
        assert_eq!(r1.last_seen, "sha256:b");
        assert_eq!(r1.page_uuid.as_deref(), Some("page-1"));
    }

    #[test]
    fn designator_reuse_is_marked_as_drift_and_keeps_history() {
        // R1 第一次观测映射 prim-r1，第二次观测位号复用映射到 prim-r9：
        // 同一逻辑 ID 的图元集合跨观测不同 → drifted，且保留全部历史映射。
        let observations = vec![
            observation(
                "sha256:a",
                "2026-10-22T08:00:00.000Z",
                json!([component_ref("component:R1", "prim-r1", "page-1")]),
            ),
            observation(
                "sha256:b",
                "2026-10-22T08:01:00.000Z",
                json!([component_ref("component:R1", "prim-r9", "page-1")]),
            ),
        ];
        let view = build_fusion_view(&observations);

        assert!(view.conflicts.is_empty());
        let r1 = view.mapping_for("component:R1").expect("R1 映射");
        assert_eq!(r1.status, MappingStatus::Drifted);
        assert_eq!(r1.primitive_ids, vec!["prim-r1", "prim-r9"]);
        assert!(r1.present_in_latest);
    }

    #[test]
    fn double_owned_primitive_in_one_observation_is_a_conflict() {
        let observations = vec![observation(
            "sha256:a",
            "2026-10-22T08:00:00.000Z",
            json!([
                component_ref("component:R1", "prim-x", "page-1"),
                component_ref("component:R2", "prim-x", "page-1"),
            ]),
        )];
        let view = build_fusion_view(&observations);

        assert_eq!(view.conflicts.len(), 1);
        let conflict = &view.conflicts[0];
        assert_eq!(conflict.primitive_id.as_deref(), Some("prim-x"));
        assert_eq!(conflict.logical_ids, vec!["component:R1", "component:R2"]);
        assert!(conflict.description.contains("prim-x"));
        assert_eq!(view.mapping_for("component:R1").expect("R1").status, MappingStatus::Conflicted);
        assert_eq!(view.mapping_for("component:R2").expect("R2").status, MappingStatus::Conflicted);
    }

    #[test]
    fn missing_in_latest_keeps_mapping_but_marks_absence() {
        let observations = vec![
            observation(
                "sha256:a",
                "2026-10-22T08:00:00.000Z",
                json!([
                    component_ref("component:R1", "prim-r1", "page-1"),
                    component_ref("component:R2", "prim-r2", "page-1"),
                ]),
            ),
            observation(
                "sha256:b",
                "2026-10-22T08:01:00.000Z",
                json!([component_ref("component:R1", "prim-r1", "page-1")]),
            ),
        ];
        let view = build_fusion_view(&observations);

        let r2 = view.mapping_for("component:R2").expect("R2 历史映射保留");
        assert!(!r2.present_in_latest, "不得假装 R2 仍存在于最新观测");
        assert_eq!(r2.status, MappingStatus::Stable);
        assert_eq!(r2.last_seen, "sha256:a");
        let r1 = view.mapping_for("component:R1").expect("R1 映射");
        assert!(r1.present_in_latest);
    }

    #[test]
    fn empty_observations_yield_an_empty_view() {
        let view = build_fusion_view(&[]);
        assert!(view.mappings.is_empty());
        assert!(view.conflicts.is_empty());
    }

    #[test]
    fn every_duplicate_logical_identity_is_conflicted() {
        let view = build_fusion_view(&[observation(
            "hash",
            "now",
            json!([
                component_ref("component:R1", "a", "page-1"),
                component_ref("component:R1", "b", "page-1"),
                component_ref("component:R2", "c", "page-1"),
                component_ref("component:R2", "d", "page-1"),
            ]),
        )]);
        assert_eq!(view.conflicts.len(), 2);
        for id in ["component:R1", "component:R2"] {
            assert_eq!(view.mapping_for(id).unwrap().status, MappingStatus::Conflicted);
        }
    }

    #[test]
    fn page_change_is_drift_even_when_primitive_id_is_unchanged() {
        let view = build_fusion_view(&[
            observation("a", "1", json!([component_ref("component:R1", "prim", "page-1")])),
            observation("b", "2", json!([component_ref("component:R1", "prim", "page-2")])),
        ]);
        assert_eq!(view.mapping_for("component:R1").unwrap().status, MappingStatus::Drifted);
    }

    #[test]
    fn a_consistent_re_observation_confirms_identity_after_a_change() {
        // 实测：重开工程后页面 UUID 变化，全部映射漂移；此后再观测一致即应恢复稳定，
        // 且只暴露当前图元，不把历史图元当作写入目标。
        let view = build_fusion_view(&[
            observation("a", "1", json!([component_ref("component:U1", "prim-u1", "page-old")])),
            observation("b", "2", json!([component_ref("component:U1", "prim-u1", "page-new")])),
            observation("c", "3", json!([component_ref("component:U1", "prim-u1", "page-new")])),
        ]);
        let u1 = view.mapping_for("component:U1").unwrap();
        assert_eq!(u1.status, MappingStatus::Stable);
        assert_eq!(u1.primitive_ids, vec!["prim-u1"]);

        // 位号换到另一个图元：变化后的第一次观测仍是漂移，再确认一次才稳定。
        let reused = [
            observation("a", "1", json!([component_ref("component:R1", "prim-r1", "page-1")])),
            observation("b", "2", json!([component_ref("component:R1", "prim-r9", "page-1")])),
            observation("c", "3", json!([component_ref("component:R1", "prim-r9", "page-1")])),
        ];
        assert_eq!(build_fusion_view(&reused[..2]).mapping_for("component:R1").unwrap().status,
            MappingStatus::Drifted);
        let r1 = build_fusion_view(&reused).mapping_for("component:R1").unwrap().clone();
        assert_eq!(r1.status, MappingStatus::Stable);
        assert_eq!(r1.primitive_ids, vec!["prim-r9"]);

        // 消失后再出现（如撤销恢复）也需再确认一次。
        let returned = [
            observation("a", "1", json!([component_ref("component:U1", "prim-u1", "page-1")])),
            observation("b", "2", json!([])),
            observation("c", "3", json!([component_ref("component:U1", "prim-u1", "page-1")])),
        ];
        assert_eq!(build_fusion_view(&returned).mapping_for("component:U1").unwrap().status,
            MappingStatus::Drifted);
    }

    #[test]
    fn resolved_historical_conflicts_do_not_block_forever() {
        let view = build_fusion_view(&[
            observation("a", "1", json!([
                component_ref("component:R1", "prim-x", "page-1"),
                component_ref("component:R2", "prim-x", "page-1"),
            ])),
            observation("b", "2", json!([
                component_ref("component:R1", "prim-r1", "page-1"),
                component_ref("component:R2", "prim-x", "page-1"),
            ])),
            observation("c", "3", json!([
                component_ref("component:R1", "prim-r1", "page-1"),
                component_ref("component:R2", "prim-x", "page-1"),
            ])),
        ]);
        assert_eq!(view.conflicts.len(), 1, "历史冲突仍保留可审计");
        for id in ["component:R1", "component:R2"] {
            assert_eq!(view.mapping_for(id).unwrap().status, MappingStatus::Stable, "{id}");
        }
    }

    #[test]
    fn primitive_ids_are_scoped_to_the_page_and_historical_reassignment_drifts() {
        let view = build_fusion_view(&[observation(
            "a",
            "1",
            json!([
                component_ref("component:R1", "prim", "page-1"),
                component_ref("component:R2", "prim", "page-2"),
            ]),
        )]);
        assert!(view.conflicts.is_empty());
        assert!(view.mappings.iter().all(|mapping| mapping.status.is_stable()));

        let view = build_fusion_view(&[
            observation("a", "1", json!([component_ref("component:R1", "prim", "page-1")])),
            observation("b", "2", json!([component_ref("component:R9", "prim", "page-1")])),
        ]);
        assert!(view.conflicts.is_empty());
        for mapping in view.mappings {
            assert_eq!(mapping.status, MappingStatus::Drifted);
            assert_ne!(mapping.logical_id, "prim", "外部 ID 不得成为逻辑身份");
        }
    }

    #[test]
    fn blank_external_ids_are_not_stable_mappings() {
        let view = build_fusion_view(&[observation(
            "a",
            "1",
            json!([component_ref("component:R1", "  ", "page-1"), net_ref("net:VCC", &[""]),]),
        )]);
        assert!(view.mappings.iter().all(|mapping| !mapping.status.is_stable()));
    }

    #[test]
    fn rebuild_and_persist_roundtrips() {
        let dir = std::env::temp_dir().join(format!(
            "circuitfabric-fusion-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos()
        ));
        let root = dir.join("project");
        let record = json!({
            "schemaVersion": 1,
            "kind": "inspect",
            "projectId": "p1",
            "sessionId": "s1",
            "capturedAt": "2026-10-22T08:00:00.000Z",
            "receivedAt": "2026-10-22T08:00:01.000Z",
            "snapshot": circuitfabric_core::stamp_snapshot(&LogicalCircuitSnapshot::empty(
                circuitfabric_contracts::SnapshotAuthority::Observed)).unwrap(),
            "externalRefs": [component_ref("component:R1", "prim-r1", "page-1")],
            "inconclusive": [],
            "validation": [],
            "physicalProjection": {},
        });
        crate::observation::persist_inspect_record(&root, &record).expect("落盘观测记录");

        let (path, view) = rebuild_and_persist(&root).expect("重建融合视图");
        assert!(path.is_file());
        assert_eq!(view.mappings.len(), 1);
        let parsed: Value =
            serde_json::from_slice(&std::fs::read(&path).expect("读取")).expect("解析");
        assert_eq!(parsed["schemaVersion"], 1);
        assert_eq!(parsed["mappings"][0]["logicalId"], "component:R1");
        assert_eq!(parsed["mappings"][0]["status"], "stable");
        assert_eq!(parsed["mappings"][0]["presentInLatest"], true);

        std::fs::remove_dir_all(&dir).ok();
    }
}
