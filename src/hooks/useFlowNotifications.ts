/**
 * Flow 通知 Hook
 *
 * 集成通知服务与 Flow 事件，提供实时通知功能。
 *
 * **Validates: Requirements 10.1, 10.2**
 */

import { useEffect, useCallback } from "react";
import {
  notificationService,
  type NotificationPermission,
} from "@/lib/notificationService";
import { useFlowEvents } from "./useFlowEvents";
import type {
  FlowSummary,
  FlowError,
  ThresholdCheckResult,
} from "@/lib/api/flowMonitor";

interface UseFlowNotificationsOptions {
  /** 是否启用通知 */
  enabled?: boolean;
  /** 是否自动请求权限 */
  autoRequestPermission?: boolean;
}

interface UseFlowNotificationsReturn {
  /** 通知服务实例 */
  notificationService: typeof notificationService;
  /** 请求通知权限 */
  requestPermission: () => Promise<boolean>;
  /** 权限状态 */
  permissionStatus: NotificationPermission;
  /** 是否可以发送通知 */
  canNotify: boolean;
}

/**
 * Flow 通知 Hook
 *
 * 自动监听 Flow 事件并发送相应的通知。
 */
export function useFlowNotifications(
  options: UseFlowNotificationsOptions = {},
): UseFlowNotificationsReturn {
  const { enabled = true, autoRequestPermission = false } = options;

  // 处理 Flow 开始事件
  const handleFlowStarted = useCallback(
    (flow: FlowSummary) => {
      if (!enabled) return;

      notificationService.notifyNewFlow(flow.model, flow.provider);
    },
    [enabled],
  );

  // 处理 Flow 失败事件
  const handleFlowFailed = useCallback(
    (id: string, error: FlowError) => {
      if (!enabled) return;

      notificationService.notifyError(
        id,
        error.message || `${error.error_type} 错误`,
      );
    },
    [enabled],
  );

  // 处理阈值警告事件
  const handleThresholdWarning = useCallback(
    (id: string, result: ThresholdCheckResult) => {
      if (!enabled) return;

      const warnings = {
        latencyExceeded: result.latency_exceeded,
        tokenExceeded:
          result.token_exceeded ||
          result.input_token_exceeded ||
          result.output_token_exceeded,
        actualLatency: result.actual_latency_ms,
        actualTokens:
          result.actual_tokens ||
          result.actual_input_tokens ||
          result.actual_output_tokens,
      };

      notificationService.notifyThresholdWarning(id, warnings);
    },
    [enabled],
  );

  // 使用 Flow 事件 Hook
  useFlowEvents({
    autoConnect: enabled,
    onFlowStarted: handleFlowStarted,
    onFlowFailed: handleFlowFailed,
    onThresholdWarning: handleThresholdWarning,
  });

  // 请求权限
  const requestPermission = useCallback(async () => {
    return await notificationService.requestPermission();
  }, []);

  // 自动请求权限
  useEffect(() => {
    if (enabled && autoRequestPermission) {
      const permission = notificationService.getPermissionStatus();
      if (permission === "default") {
        requestPermission();
      }
    }
  }, [enabled, autoRequestPermission, requestPermission]);

  return {
    notificationService,
    requestPermission,
    permissionStatus: notificationService.getPermissionStatus(),
    canNotify: notificationService.canNotify(),
  };
}

export default useFlowNotifications;
