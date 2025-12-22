//! 批量操作服务
//!
//! 该模块实现 Flow 批量操作功能，支持对多个 Flow 进行批量收藏、
//! 添加标签、导出、删除等操作。
//!
//! **Validates: Requirements 11.2-11.6**

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;

use super::exporter::{ExportFormat, ExportOptions, FlowExporter};
use super::models::LLMFlow;
use super::monitor::FlowMonitor;
use super::session::SessionManager;

/// 批量操作错误
#[derive(Debug, Error)]
pub enum BatchOpsError {
    #[error("Flow 不存在: {0}")]
    FlowNotFound(String),
    #[error("会话不存在: {0}")]
    SessionNotFound(String),
    #[error("导出错误: {0}")]
    ExportError(String),
    #[error("操作失败: {0}")]
    OperationFailed(String),
}

pub type Result<T> = std::result::Result<T, BatchOpsError>;

/// 批量操作类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BatchOperation {
    Star,
    Unstar,
    AddTags { tags: Vec<String> },
    RemoveTags { tags: Vec<String> },
    Export { format: ExportFormat },
    Delete,
    AddToSession { session_id: String },
}

/// 批量操作结果
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BatchResult {
    pub total: usize,
    pub success: usize,
    pub failed: usize,
    pub errors: Vec<(String, String)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub export_data: Option<String>,
}

impl BatchResult {
    pub fn new(total: usize) -> Self {
        Self {
            total,
            success: 0,
            failed: 0,
            errors: Vec::new(),
            export_data: None,
        }
    }
    pub fn record_success(&mut self) {
        self.success += 1;
    }
    pub fn record_failure(&mut self, flow_id: impl Into<String>, error: impl Into<String>) {
        self.failed += 1;
        self.errors.push((flow_id.into(), error.into()));
    }
    pub fn is_all_success(&self) -> bool {
        self.failed == 0
    }
    pub fn is_all_failed(&self) -> bool {
        self.success == 0 && self.total > 0
    }
    pub fn is_partial_success(&self) -> bool {
        self.success > 0 && self.failed > 0
    }
}

/// 批量操作服务
pub struct BatchOperations {
    flow_monitor: Arc<FlowMonitor>,
    session_manager: Option<Arc<SessionManager>>,
}

impl BatchOperations {
    pub fn new(
        flow_monitor: Arc<FlowMonitor>,
        session_manager: Option<Arc<SessionManager>>,
    ) -> Self {
        Self {
            flow_monitor,
            session_manager,
        }
    }

    pub async fn execute(&self, flow_ids: &[String], operation: BatchOperation) -> BatchResult {
        self.execute_with_progress(flow_ids, operation, |_, _| {})
            .await
    }

    pub async fn execute_with_progress<F>(
        &self,
        flow_ids: &[String],
        operation: BatchOperation,
        progress: F,
    ) -> BatchResult
    where
        F: Fn(usize, usize) + Send + Sync,
    {
        let mut result = BatchResult::new(flow_ids.len());
        match operation {
            BatchOperation::Star => {
                self.batch_star(flow_ids, true, &mut result, &progress)
                    .await
            }
            BatchOperation::Unstar => {
                self.batch_star(flow_ids, false, &mut result, &progress)
                    .await
            }
            BatchOperation::AddTags { tags } => {
                self.batch_add_tags(flow_ids, &tags, &mut result, &progress)
                    .await
            }
            BatchOperation::RemoveTags { tags } => {
                self.batch_remove_tags(flow_ids, &tags, &mut result, &progress)
                    .await
            }
            BatchOperation::Export { format } => {
                self.batch_export(flow_ids, format, &mut result, &progress)
                    .await
            }
            BatchOperation::Delete => self.batch_delete(flow_ids, &mut result, &progress).await,
            BatchOperation::AddToSession { session_id } => {
                self.batch_add_to_session(flow_ids, &session_id, &mut result, &progress)
                    .await
            }
        }
        result
    }

    async fn batch_star<F>(
        &self,
        flow_ids: &[String],
        starred: bool,
        result: &mut BatchResult,
        progress: &F,
    ) where
        F: Fn(usize, usize),
    {
        let total = flow_ids.len();
        for (i, flow_id) in flow_ids.iter().enumerate() {
            progress(i + 1, total);
            let memory_store = self.flow_monitor.memory_store();
            let store = memory_store.read().await;
            let current_starred = store
                .get(flow_id)
                .and_then(|f| f.read().ok().map(|flow| flow.annotations.starred));
            drop(store);
            match current_starred {
                Some(current) if current != starred => {
                    if self.flow_monitor.toggle_starred(flow_id).await {
                        result.record_success();
                    } else {
                        result.record_failure(flow_id, "更新收藏状态失败");
                    }
                }
                Some(_) => {
                    result.record_success();
                }
                None => {
                    result.record_failure(flow_id, "Flow 不存在");
                }
            }
        }
    }

    async fn batch_add_tags<F>(
        &self,
        flow_ids: &[String],
        tags: &[String],
        result: &mut BatchResult,
        progress: &F,
    ) where
        F: Fn(usize, usize),
    {
        let total = flow_ids.len();
        for (i, flow_id) in flow_ids.iter().enumerate() {
            progress(i + 1, total);
            let memory_store = self.flow_monitor.memory_store();
            let store = memory_store.read().await;
            let exists = store.get(flow_id).is_some();
            drop(store);
            if !exists {
                result.record_failure(flow_id, "Flow 不存在");
                continue;
            }
            let mut all_success = true;
            for tag in tags {
                if !self.flow_monitor.add_tag(flow_id, tag.clone()).await {
                    all_success = false;
                    break;
                }
            }
            if all_success {
                result.record_success();
            } else {
                result.record_failure(flow_id, "添加标签失败");
            }
        }
    }

    async fn batch_remove_tags<F>(
        &self,
        flow_ids: &[String],
        tags: &[String],
        result: &mut BatchResult,
        progress: &F,
    ) where
        F: Fn(usize, usize),
    {
        let total = flow_ids.len();
        for (i, flow_id) in flow_ids.iter().enumerate() {
            progress(i + 1, total);
            let memory_store = self.flow_monitor.memory_store();
            let store = memory_store.read().await;
            let exists = store.get(flow_id).is_some();
            drop(store);
            if !exists {
                result.record_failure(flow_id, "Flow 不存在");
                continue;
            }
            for tag in tags {
                let _ = self.flow_monitor.remove_tag(flow_id, tag).await;
            }
            result.record_success();
        }
    }

    async fn batch_export<F>(
        &self,
        flow_ids: &[String],
        format: ExportFormat,
        result: &mut BatchResult,
        progress: &F,
    ) where
        F: Fn(usize, usize),
    {
        let total = flow_ids.len();
        let mut flows: Vec<LLMFlow> = Vec::with_capacity(total);
        for (i, flow_id) in flow_ids.iter().enumerate() {
            progress(i + 1, total);
            let memory_store = self.flow_monitor.memory_store();
            let store = memory_store.read().await;
            if let Some(flow_lock) = store.get(flow_id) {
                if let Ok(flow) = flow_lock.read() {
                    flows.push(flow.clone());
                    result.record_success();
                } else {
                    result.record_failure(flow_id, "无法读取 Flow");
                }
            } else {
                result.record_failure(flow_id, "Flow 不存在");
            }
        }
        if !flows.is_empty() {
            let options = ExportOptions {
                format,
                ..Default::default()
            };
            let exporter = FlowExporter::new(options);
            let export_result = exporter.export(&flows);
            result.export_data = Some(export_result.to_string_pretty());
        }
    }

    async fn batch_delete<F>(&self, flow_ids: &[String], result: &mut BatchResult, progress: &F)
    where
        F: Fn(usize, usize),
    {
        let total = flow_ids.len();
        for (i, flow_id) in flow_ids.iter().enumerate() {
            progress(i + 1, total);
            let memory_store = self.flow_monitor.memory_store();
            let mut store = memory_store.write().await;
            if store.remove(flow_id) {
                result.record_success();
            } else {
                result.record_failure(flow_id, "Flow 不存在或删除失败");
            }
        }
    }

    async fn batch_add_to_session<F>(
        &self,
        flow_ids: &[String],
        session_id: &str,
        result: &mut BatchResult,
        progress: &F,
    ) where
        F: Fn(usize, usize),
    {
        let total = flow_ids.len();
        let session_manager = match &self.session_manager {
            Some(sm) => sm,
            None => {
                for flow_id in flow_ids {
                    result.record_failure(flow_id, "会话管理器不可用");
                }
                return;
            }
        };
        match session_manager.get_session(session_id) {
            Ok(Some(_)) => {}
            Ok(None) => {
                for flow_id in flow_ids {
                    result.record_failure(flow_id, format!("会话不存在: {}", session_id));
                }
                return;
            }
            Err(e) => {
                for flow_id in flow_ids {
                    result.record_failure(flow_id, format!("查询会话失败: {}", e));
                }
                return;
            }
        }
        for (i, flow_id) in flow_ids.iter().enumerate() {
            progress(i + 1, total);
            let memory_store = self.flow_monitor.memory_store();
            let store = memory_store.read().await;
            let exists = store.get(flow_id).is_some();
            drop(store);
            if !exists {
                result.record_failure(flow_id, "Flow 不存在");
                continue;
            }
            match session_manager.add_flow(session_id, flow_id) {
                Ok(_) => {
                    result.record_success();
                }
                Err(e) => {
                    result.record_failure(flow_id, format!("添加到会话失败: {}", e));
                }
            }
        }
    }
}

// ============================================================================
// 属性测试
// ============================================================================

#[cfg(test)]
mod property_tests {
    use super::*;
    use crate::flow_monitor::models::{FlowMetadata, FlowType, LLMRequest};
    use crate::flow_monitor::monitor::FlowMonitorConfig;
    use proptest::prelude::*;

    fn create_test_flow_monitor() -> Arc<FlowMonitor> {
        let config = FlowMonitorConfig::default();
        Arc::new(FlowMonitor::new(config, None))
    }

    async fn create_test_flow(monitor: &FlowMonitor, flow_id: &str) -> String {
        let request = LLMRequest {
            method: "POST".to_string(),
            path: "/v1/chat/completions".to_string(),
            model: "gpt-4".to_string(),
            ..Default::default()
        };
        let metadata = FlowMetadata::default();
        let mut flow = crate::flow_monitor::models::LLMFlow::new(
            flow_id.to_string(),
            FlowType::ChatCompletions,
            request,
            metadata,
        );
        flow.state = crate::flow_monitor::models::FlowState::Completed;
        let store = monitor.memory_store();
        store.write().await.add(flow);
        flow_id.to_string()
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Feature: flow-monitor-enhancement, Property 20: 批量操作正确性**
        /// **Validates: Requirements 11.2-11.6**
        ///
        /// *对于任意* Flow 集合和批量操作，操作后所有 Flow 应该被正确更新。
        #[test]
        fn prop_batch_star_correctness(flow_count in 1usize..10usize) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let monitor = create_test_flow_monitor();
                let batch_ops = BatchOperations::new(monitor.clone(), None);

                // 创建测试 Flow
                let mut flow_ids = Vec::new();
                for i in 0..flow_count {
                    let id = create_test_flow(&monitor, &format!("flow-{}", i)).await;
                    flow_ids.push(id);
                }

                // 执行批量收藏
                let result = batch_ops.execute(&flow_ids, BatchOperation::Star).await;

                // 验证结果
                prop_assert_eq!(result.total, flow_count);
                prop_assert_eq!(result.success, flow_count);
                prop_assert_eq!(result.failed, 0);

                // 验证所有 Flow 都被收藏
                let store = monitor.memory_store();
                let s = store.read().await;
                for flow_id in &flow_ids {
                    if let Some(flow_lock) = s.get(flow_id) {
                        let flow = flow_lock.read().unwrap();
                        prop_assert!(flow.annotations.starred, "Flow {} 应该被收藏", flow_id);
                    }
                }
                Ok(())
            })?;
        }

        /// **Feature: flow-monitor-enhancement, Property 21: 批量操作原子性**
        /// **Validates: Requirements 11.2-11.6**
        ///
        /// *对于任意* 批量操作，如果部分失败，成功的部分应该被正确应用，失败的部分应该被正确报告。
        #[test]
        fn prop_batch_operation_atomicity(
            valid_flow_count in 1usize..8usize,
            invalid_flow_count in 1usize..5usize,
        ) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let monitor = create_test_flow_monitor();
                let batch_ops = BatchOperations::new(monitor.clone(), None);

                // 创建有效的 Flow
                let mut valid_flow_ids = Vec::new();
                for i in 0..valid_flow_count {
                    let id = create_test_flow(&monitor, &format!("valid-flow-{}", i)).await;
                    valid_flow_ids.push(id);
                }

                // 创建无效的 Flow ID（不存在的）
                let mut invalid_flow_ids = Vec::new();
                for i in 0..invalid_flow_count {
                    invalid_flow_ids.push(format!("invalid-flow-{}", i));
                }

                // 混合有效和无效的 Flow ID
                let mut all_flow_ids = valid_flow_ids.clone();
                all_flow_ids.extend(invalid_flow_ids.clone());

                // 执行批量收藏操作
                let result = batch_ops.execute(&all_flow_ids, BatchOperation::Star).await;

                // 验证结果统计
                prop_assert_eq!(result.total, valid_flow_count + invalid_flow_count);
                prop_assert_eq!(result.success, valid_flow_count);
                prop_assert_eq!(result.failed, invalid_flow_count);
                prop_assert_eq!(result.errors.len(), invalid_flow_count);

                // 验证成功的 Flow 被正确更新
                let store = monitor.memory_store();
                let s = store.read().await;
                for flow_id in &valid_flow_ids {
                    if let Some(flow_lock) = s.get(flow_id) {
                        let flow = flow_lock.read().unwrap();
                        prop_assert!(flow.annotations.starred, "有效的 Flow {} 应该被收藏", flow_id);
                    }
                }

                // 验证失败的 Flow ID 被正确报告
                for invalid_id in &invalid_flow_ids {
                    let found_error = result.errors.iter().any(|(id, _)| id == invalid_id);
                    prop_assert!(found_error, "无效的 Flow ID {} 应该在错误列表中", invalid_id);
                }

                // 验证部分成功状态
                prop_assert!(result.is_partial_success(), "应该是部分成功状态");
                prop_assert!(!result.is_all_success(), "不应该是全部成功");
                prop_assert!(!result.is_all_failed(), "不应该是全部失败");

                Ok(())
            })?;
        }

        /// **Feature: flow-monitor-enhancement, Property 21b: 批量标签操作原子性**
        /// **Validates: Requirements 11.2-11.6**
        ///
        /// *对于任意* 批量标签操作，部分失败时应该正确处理成功和失败的情况。
        #[test]
        fn prop_batch_tag_operation_atomicity(
            valid_flow_count in 1usize..6usize,
            invalid_flow_count in 1usize..4usize,
            tag_count in 1usize..4usize,
        ) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let monitor = create_test_flow_monitor();
                let batch_ops = BatchOperations::new(monitor.clone(), None);

                // 创建有效的 Flow
                let mut valid_flow_ids = Vec::new();
                for i in 0..valid_flow_count {
                    let id = create_test_flow(&monitor, &format!("valid-flow-{}", i)).await;
                    valid_flow_ids.push(id);
                }

                // 创建无效的 Flow ID
                let mut invalid_flow_ids = Vec::new();
                for i in 0..invalid_flow_count {
                    invalid_flow_ids.push(format!("invalid-flow-{}", i));
                }

                // 创建标签列表
                let tags: Vec<String> = (0..tag_count).map(|i| format!("tag-{}", i)).collect();

                // 混合有效和无效的 Flow ID
                let mut all_flow_ids = valid_flow_ids.clone();
                all_flow_ids.extend(invalid_flow_ids.clone());

                // 执行批量添加标签操作
                let result = batch_ops.execute(
                    &all_flow_ids,
                    BatchOperation::AddTags { tags: tags.clone() }
                ).await;

                // 验证结果统计
                prop_assert_eq!(result.total, valid_flow_count + invalid_flow_count);
                prop_assert_eq!(result.success, valid_flow_count);
                prop_assert_eq!(result.failed, invalid_flow_count);

                // 验证成功的 Flow 被正确添加标签
                let store = monitor.memory_store();
                let s = store.read().await;
                for flow_id in &valid_flow_ids {
                    if let Some(flow_lock) = s.get(flow_id) {
                        let flow = flow_lock.read().unwrap();
                        for tag in &tags {
                            prop_assert!(
                                flow.annotations.tags.contains(tag),
                                "有效的 Flow {} 应该包含标签 {}",
                                flow_id,
                                tag
                            );
                        }
                    }
                }

                // 验证失败的 Flow ID 被正确报告
                for invalid_id in &invalid_flow_ids {
                    let found_error = result.errors.iter().any(|(id, _)| id == invalid_id);
                    prop_assert!(found_error, "无效的 Flow ID {} 应该在错误列表中", invalid_id);
                }

                Ok(())
            })?;
        }
    }
}
