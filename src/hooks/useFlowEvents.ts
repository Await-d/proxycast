import { useState, useEffect, useCallback, useRef } from "react";
import { flowEventManager } from "@/lib/flowEventManager";
import type {
  FlowEvent,
  FlowSummary,
  FlowUpdate,
  FlowError,
  ThresholdCheckResult,
} from "@/lib/api/flowMonitor";

interface UseFlowEventsOptions {
  /** 是否自动连接 */
  autoConnect?: boolean;
  /** 事件回调 */
  onFlowStarted?: (flow: FlowSummary) => void;
  onFlowUpdated?: (id: string, update: FlowUpdate) => void;
  onFlowCompleted?: (id: string, summary: FlowSummary) => void;
  onFlowFailed?: (id: string, error: FlowError) => void;
  onThresholdWarning?: (id: string, result: ThresholdCheckResult) => void;
}

interface UseFlowEventsReturn {
  /** 连接状态 */
  connected: boolean;
  /** 是否正在连接 */
  connecting: boolean;
  /** 错误信息 */
  error: string | null;
  /** 手动连接 */
  connect: () => void;
  /** 手动断开 */
  disconnect: () => void;
  /** 最近的事件 */
  lastEvent: FlowEvent | null;
  /** 活跃的 Flow 列表（正在进行中的） */
  activeFlows: Map<string, FlowSummary>;
  /** 最近的阈值警告 */
  lastThresholdWarning: { id: string; result: ThresholdCheckResult } | null;
}

/**
 * Flow 事件订阅 Hook
 *
 * 使用全局 FlowEventManager 订阅 Flow 实时事件，
 * 页面切换时不会丢失状态。
 */
export function useFlowEvents(
  options: UseFlowEventsOptions = {},
): UseFlowEventsReturn {
  const {
    autoConnect = true,
    onFlowStarted,
    onFlowUpdated,
    onFlowCompleted,
    onFlowFailed,
    onThresholdWarning,
  } = options;

  // 从全局管理器获取初始状态
  const initialState = flowEventManager.getState();

  const [connected, setConnected] = useState(initialState.subscribed);
  const [connecting, setConnecting] = useState(initialState.subscribing);
  const [error, setError] = useState<string | null>(initialState.error);
  const [lastEvent, setLastEvent] = useState<FlowEvent | null>(
    initialState.lastEvent,
  );
  const [activeFlows, setActiveFlows] = useState<Map<string, FlowSummary>>(
    initialState.activeFlows,
  );
  const [lastThresholdWarning, setLastThresholdWarning] = useState<{
    id: string;
    result: ThresholdCheckResult;
  } | null>(null);

  const callbacksRef = useRef({
    onFlowStarted,
    onFlowUpdated,
    onFlowCompleted,
    onFlowFailed,
    onThresholdWarning,
  });

  // 更新回调引用
  useEffect(() => {
    callbacksRef.current = {
      onFlowStarted,
      onFlowUpdated,
      onFlowCompleted,
      onFlowFailed,
      onThresholdWarning,
    };
  }, [
    onFlowStarted,
    onFlowUpdated,
    onFlowCompleted,
    onFlowFailed,
    onThresholdWarning,
  ]);

  // 处理事件
  const handleEvent = useCallback((event: FlowEvent) => {
    setLastEvent(event);

    // 更新活跃 Flow 状态
    switch (event.type) {
      case "FlowStarted":
        setActiveFlows((prev) => {
          const next = new Map(prev);
          next.set(event.flow.id, event.flow);
          return next;
        });
        callbacksRef.current.onFlowStarted?.(event.flow);
        break;

      case "FlowUpdated":
        setActiveFlows((prev) => {
          const next = new Map(prev);
          const existing = next.get(event.id);
          if (existing && event.update.state) {
            next.set(event.id, { ...existing, state: event.update.state });
          }
          return next;
        });
        callbacksRef.current.onFlowUpdated?.(event.id, event.update);
        break;

      case "ThresholdWarning":
        setLastThresholdWarning({ id: event.id, result: event.result });
        callbacksRef.current.onThresholdWarning?.(event.id, event.result);
        break;

      case "FlowCompleted":
        setActiveFlows((prev) => {
          const next = new Map(prev);
          next.delete(event.id);
          return next;
        });
        callbacksRef.current.onFlowCompleted?.(event.id, event.summary);
        break;

      case "FlowFailed":
        setActiveFlows((prev) => {
          const next = new Map(prev);
          next.delete(event.id);
          return next;
        });
        callbacksRef.current.onFlowFailed?.(event.id, event.error);
        break;
    }
  }, []);

  // 连接
  const connect = useCallback(async () => {
    if (flowEventManager.isSubscribed()) {
      setConnected(true);
      setConnecting(false);
      return;
    }

    setConnecting(true);
    setError(null);

    await flowEventManager.subscribe();

    const state = flowEventManager.getState();
    setConnected(state.subscribed);
    setConnecting(state.subscribing);
    setError(state.error);
    setActiveFlows(state.activeFlows);
    setLastEvent(state.lastEvent);
  }, []);

  // 断开（通常不需要调用，因为是全局订阅）
  const disconnect = useCallback(() => {
    // 注意：这会影响所有使用 useFlowEvents 的组件
    // 通常不应该调用这个方法
    flowEventManager.unsubscribe();
    setConnected(false);
    setConnecting(false);
  }, []);

  // 注册回调并自动连接
  useEffect(() => {
    // 注册事件回调
    const removeCallback = flowEventManager.addCallback(handleEvent);

    // 自动连接
    if (autoConnect) {
      connect();
    }

    // 同步状态
    const state = flowEventManager.getState();
    setConnected(state.subscribed);
    setConnecting(state.subscribing);
    setError(state.error);
    setActiveFlows(state.activeFlows);
    setLastEvent(state.lastEvent);

    return () => {
      // 只移除回调，不取消订阅（保持全局订阅）
      removeCallback();
    };
  }, [autoConnect, connect, handleEvent]);

  return {
    connected,
    connecting,
    error,
    connect,
    disconnect,
    lastEvent,
    activeFlows,
    lastThresholdWarning,
  };
}

export default useFlowEvents;
