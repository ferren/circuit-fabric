//! Desktop use cases and read models. No GPUI types belong in this layer.
pub mod navigation;
pub mod overview;
pub mod plugin_governance;
pub mod project_data;
pub mod project_runtime;
pub mod runtime_session;
pub mod settings_persistence;
pub mod usage_audit;

#[cfg(test)]
mod project_data_tests;
pub mod semantic_query;
