//! Bridge 操作协议 v1 的版本化消息类型。
//!
//! 这些类型是 `docs/bridge-operations-protocol.zh-CN.md` 中契约的 Rust 表示：
//! 会话通道（`/bridge`）接收操作请求，执行器通道（`/eda-operations`）由 EDA
//! 扩展入口代理执行真实 API 调用。消息字段名与协议文档逐字一致。

use circuitfabric_plugin_api::Capability;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 协议版本；握手与每条 `eda_operation` 均携带此值。
pub const OPERATIONS_PROTOCOL_VERSION: u64 = 1;

/// v1 已实现并通过测试的操作；`status`/`hello_ack` 只上报这个集合。
pub const IMPLEMENTED_OPERATIONS: &[&str] = &[
    "inspect",
    "read_context",
    "drc",
    "visual_capture",
    "preview",
    "apply",
    "readback",
    "rollback",
];

/// 在 EDA 运行时执行的子集；preview 在 Bridge 本地执行。
pub const EXECUTOR_OPERATIONS: &[&str] =
    &["inspect", "read_context", "drc", "visual_capture", "apply", "readback", "rollback"];

/// 未实现操作集合；具体写入类型仍由计划前置条件限制。
pub const UNSUPPORTED_OPERATIONS: &[&str] = &[];

/// 会话通道与执行器通道共享的操作种类（wire 名为 `snake_case`）。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgeOperation {
    Inspect,
    ReadContext,
    Drc,
    VisualCapture,
    Preview,
    Apply,
    Readback,
    Rollback,
}

impl BridgeOperation {
    /// Wire 标识，与 serde 的 `snake_case` 序列化一致。
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Inspect => "inspect",
            Self::ReadContext => "read_context",
            Self::Drc => "drc",
            Self::VisualCapture => "visual_capture",
            Self::Preview => "preview",
            Self::Apply => "apply",
            Self::Readback => "readback",
            Self::Rollback => "rollback",
        }
    }

    /// 解析 wire 标识；未知操作返回 `None`。
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "inspect" => Some(Self::Inspect),
            "read_context" => Some(Self::ReadContext),
            "drc" => Some(Self::Drc),
            "visual_capture" => Some(Self::VisualCapture),
            "preview" => Some(Self::Preview),
            "apply" => Some(Self::Apply),
            "readback" => Some(Self::Readback),
            "rollback" => Some(Self::Rollback),
            _ => None,
        }
    }

    /// v1 是否已实现该操作；preview 为 Bridge 侧纯计算，不下发执行器。
    #[must_use]
    pub fn is_implemented(&self) -> bool {
        matches!(
            self,
            Self::Inspect
                | Self::ReadContext
                | Self::Drc
                | Self::VisualCapture
                | Self::Preview
                | Self::Apply
                | Self::Readback
                | Self::Rollback
        )
    }
}

/// v1 诚实能力集：只包含已实现并通过测试的能力。
#[must_use]
pub fn implemented_capabilities() -> Vec<Capability> {
    vec![
        Capability::Inspect,
        Capability::Preview,
        Capability::Apply,
        Capability::Readback,
        Capability::Rollback,
        Capability::Drc,
        Capability::VisualCapture,
        Capability::BridgeChat,
        Capability::BridgeContext,
    ]
}

/// Bridge → 执行器的操作请求（`eda_operation`）。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EdaOperationRequest {
    /// 消息类型，恒为 `eda_operation`（执行器按此字段分发）。
    #[serde(rename = "type", default = "eda_operation_wire_type")]
    pub message_type: String,
    pub request_id: String,
    pub operation: BridgeOperation,
    pub protocol_version: u64,
    pub project_id: String,
    pub session_id: String,
    pub document: Value,
    /// 只读操作为 `null`，物化操作绑定已审批基线。
    pub baseline_snapshot_hash: Option<String>,
    /// 只读操作为 `null`，物化操作绑定已审批计划。
    pub plan_hash: Option<String>,
    /// 权限范围；变更需要 `project.write`。
    pub scope: Vec<String>,
    pub params: Value,
}

fn eda_operation_wire_type() -> String {
    "eda_operation".to_owned()
}

/// 执行器 → Bridge 的进度事件（`operation_progress`）。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationProgress {
    pub request_id: String,
    pub operation: BridgeOperation,
    pub phase: String,
    pub completed: u64,
    pub total: u64,
}

/// 执行器 → Bridge 的结果事件（`operation_result`）。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationResult {
    pub request_id: String,
    pub operation: BridgeOperation,
    pub ok: bool,
    pub captured_at: Option<String>,
    pub payload: Value,
    /// 缺失/不可用的观测面；任何缺失字段必须出现在这里。
    pub inconclusive: Vec<String>,
}

/// 执行器 → Bridge 的错误事件（`operation_error`）。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationErrorEvent {
    pub request_id: String,
    pub operation: BridgeOperation,
    pub code: String,
    pub message: String,
    pub inconclusive: Vec<String>,
}

#[cfg(test)]
mod tests {
    use circuitfabric_plugin_api::Capability;

    use super::*;

    #[test]
    fn capabilities_are_the_honest_v1_set() {
        let capabilities = implemented_capabilities();

        assert_eq!(
            capabilities,
            vec![
                Capability::Inspect,
                Capability::Preview,
                Capability::Apply,
                Capability::Readback,
                Capability::Rollback,
                Capability::Drc,
                Capability::VisualCapture,
                Capability::BridgeChat,
                Capability::BridgeContext,
            ]
        );
        for capability in [Capability::Apply, Capability::Readback, Capability::Rollback] {
            assert!(capabilities.contains(&capability), "{capability:?} 应出现在能力集中");
        }
        let manifest: Value =
            serde_json::from_str(include_str!("../plugin-manifest.json")).unwrap();
        assert_eq!(
            manifest["capabilities"],
            serde_json::json!(capabilities.iter().map(Capability::as_str).collect::<Vec<_>>())
        );
    }

    #[test]
    fn operation_wire_names_roundtrip() {
        let cases = [
            (BridgeOperation::Inspect, "inspect"),
            (BridgeOperation::ReadContext, "read_context"),
            (BridgeOperation::Drc, "drc"),
            (BridgeOperation::VisualCapture, "visual_capture"),
            (BridgeOperation::Preview, "preview"),
            (BridgeOperation::Apply, "apply"),
            (BridgeOperation::Readback, "readback"),
            (BridgeOperation::Rollback, "rollback"),
        ];
        for (operation, wire) in cases {
            assert_eq!(operation.as_str(), wire);
            assert_eq!(
                serde_json::to_string(&operation).expect("序列化").as_str(),
                format!("\"{wire}\"")
            );
            assert_eq!(BridgeOperation::parse(wire), Some(operation));
            assert_eq!(
                serde_json::from_str::<BridgeOperation>(&format!("\"{wire}\"")).expect("反序列化"),
                operation
            );
        }
        assert_eq!(BridgeOperation::parse("visual-capture"), None);
    }

    #[test]
    fn implemented_operations_match_the_honest_set() {
        assert_eq!(
            IMPLEMENTED_OPERATIONS,
            &[
                "inspect",
                "read_context",
                "drc",
                "visual_capture",
                "preview",
                "apply",
                "readback",
                "rollback"
            ]
        );
        assert!(UNSUPPORTED_OPERATIONS.is_empty());
        for operation in UNSUPPORTED_OPERATIONS {
            let parsed = BridgeOperation::parse(operation).expect("已知操作");
            assert!(!parsed.is_implemented());
        }
        for operation in IMPLEMENTED_OPERATIONS {
            assert!(BridgeOperation::parse(operation).expect("已知操作").is_implemented());
        }
    }
}
