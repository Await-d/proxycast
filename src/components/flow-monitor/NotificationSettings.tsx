/**
 * 通知设置组件
 *
 * 提供通知功能的详细配置界面。
 *
 * **Validates: Requirements 10.1, 10.2**
 */

import { useState, useEffect } from "react";
import { Bell, BellOff, Settings } from "lucide-react";
import {
  notificationService,
  type NotificationPermission,
} from "@/lib/notificationService";
import {
  NotificationApi,
  type NotificationConfig,
} from "@/lib/tauri/notificationApi";
import { cn } from "@/lib/utils";

interface NotificationSettingsProps {
  /** 是否显示设置面板 */
  open?: boolean;
  /** 关闭回调 */
  onClose?: () => void;
  /** 权限状态 */
  permissionStatus?: NotificationPermission;
  /** 请求权限回调 */
  onRequestPermission?: () => Promise<boolean>;
}

export function NotificationSettings({
  open = false,
  onClose,
  permissionStatus = "default",
  onRequestPermission,
}: NotificationSettingsProps) {
  const [config, setConfig] = useState<NotificationConfig | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // 加载配置
  useEffect(() => {
    if (open) {
      loadConfig();
    }
  }, [open]);

  const loadConfig = async () => {
    try {
      setLoading(true);
      setError(null);
      const backendConfig = await NotificationApi.getConfig();
      setConfig(backendConfig);
    } catch (e) {
      console.error("加载通知配置失败:", e);
      setError("加载配置失败");
    } finally {
      setLoading(false);
    }
  };

  // 更新配置
  const updateConfig = async (updates: Partial<NotificationConfig>) => {
    if (!config) return;

    const newConfig = { ...config, ...updates };
    setConfig(newConfig);

    try {
      await NotificationApi.updateConfig(newConfig);
    } catch (e) {
      console.error("更新通知配置失败:", e);
      setError("更新配置失败");
      // 回滚配置
      setConfig(config);
    }
  };

  // 更新通知设置
  const updateNotificationSettings = async (
    type: "new_flow" | "error_flow" | "latency_warning" | "token_warning",
    updates: Partial<NotificationConfig["new_flow"]>,
  ) => {
    if (!config) return;

    const newConfig = {
      ...config,
      [type]: { ...config[type], ...updates },
    };
    setConfig(newConfig);

    try {
      await NotificationApi.updateConfig(newConfig);
    } catch (e) {
      console.error("更新通知设置失败:", e);
      setError("更新设置失败");
      // 回滚配置
      setConfig(config);
    }
  };

  // 测试通知
  const testNotification = async () => {
    if (permissionStatus !== "granted") {
      const granted = await onRequestPermission?.();
      if (!granted) return;
    }

    await notificationService.notify({
      title: "测试通知",
      body: "这是一条测试通知，用于验证通知功能是否正常工作。",
      type: "info",
    });
  };

  if (!open) return null;

  if (loading) {
    return (
      <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
        <div className="bg-background border rounded-lg shadow-lg p-8">
          <div className="text-center">加载中...</div>
        </div>
      </div>
    );
  }

  if (error || !config) {
    return (
      <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
        <div className="bg-background border rounded-lg shadow-lg p-8">
          <div className="text-center text-red-500">
            {error || "配置加载失败"}
          </div>
          <button
            onClick={onClose}
            className="mt-4 w-full px-3 py-2 text-sm bg-muted hover:bg-muted/80 rounded"
          >
            关闭
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="bg-background border rounded-lg shadow-lg w-96 max-h-[80vh] overflow-y-auto">
        {/* 标题栏 */}
        <div className="flex items-center justify-between p-4 border-b">
          <div className="flex items-center gap-2">
            <Settings className="h-5 w-5" />
            <h3 className="font-semibold">通知设置</h3>
          </div>
          <button
            onClick={onClose}
            className="text-muted-foreground hover:text-foreground"
          >
            ✕
          </button>
        </div>

        {/* 内容 */}
        <div className="p-4 space-y-4">
          {/* 权限状态 */}
          <div className="space-y-2">
            <label className="text-sm font-medium">通知权限</label>
            <div className="flex items-center justify-between">
              <span className="text-sm text-muted-foreground">
                {permissionStatus === "granted" && "已授权"}
                {permissionStatus === "denied" && "已拒绝"}
                {permissionStatus === "default" && "未设置"}
              </span>
              {permissionStatus !== "granted" && (
                <button
                  onClick={onRequestPermission}
                  className="text-sm px-3 py-1 bg-blue-500 text-white rounded hover:bg-blue-600"
                >
                  {permissionStatus === "denied" ? "重新授权" : "请求权限"}
                </button>
              )}
            </div>
          </div>

          {/* 基本设置 */}
          <div className="space-y-3">
            <label className="text-sm font-medium">基本设置</label>

            {/* 启用通知 */}
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                {config.enabled ? (
                  <Bell className="h-4 w-4 text-blue-500" />
                ) : (
                  <BellOff className="h-4 w-4 text-muted-foreground" />
                )}
                <span className="text-sm">启用通知</span>
              </div>
              <button
                onClick={() => updateConfig({ enabled: !config.enabled })}
                className={cn(
                  "relative inline-flex h-5 w-9 items-center rounded-full transition-colors",
                  config.enabled ? "bg-blue-500" : "bg-muted",
                )}
              >
                <span
                  className={cn(
                    "inline-block h-3 w-3 transform rounded-full bg-white transition-transform",
                    config.enabled ? "translate-x-5" : "translate-x-1",
                  )}
                />
              </button>
            </div>
          </div>

          {/* 通知类型 */}
          <div className="space-y-3">
            <label className="text-sm font-medium">通知类型</label>

            {/* 新 Flow 通知 */}
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <span className="text-sm">新 LLM 请求</span>
                <button
                  onClick={() =>
                    updateNotificationSettings("new_flow", {
                      enabled: !config.new_flow.enabled,
                    })
                  }
                  className={cn(
                    "relative inline-flex h-5 w-9 items-center rounded-full transition-colors",
                    config.new_flow.enabled ? "bg-blue-500" : "bg-muted",
                  )}
                >
                  <span
                    className={cn(
                      "inline-block h-3 w-3 transform rounded-full bg-white transition-transform",
                      config.new_flow.enabled
                        ? "translate-x-5"
                        : "translate-x-1",
                    )}
                  />
                </button>
              </div>
              {config.new_flow.enabled && (
                <div className="ml-4 space-y-1">
                  <div className="flex items-center justify-between text-xs">
                    <span>桌面通知</span>
                    <button
                      onClick={() =>
                        updateNotificationSettings("new_flow", {
                          desktop: !config.new_flow.desktop,
                        })
                      }
                      className={cn(
                        "relative inline-flex h-4 w-7 items-center rounded-full transition-colors",
                        config.new_flow.desktop ? "bg-blue-500" : "bg-muted",
                      )}
                    >
                      <span
                        className={cn(
                          "inline-block h-2 w-2 transform rounded-full bg-white transition-transform",
                          config.new_flow.desktop
                            ? "translate-x-4"
                            : "translate-x-1",
                        )}
                      />
                    </button>
                  </div>
                  <div className="flex items-center justify-between text-xs">
                    <span>声音提示</span>
                    <button
                      onClick={() =>
                        updateNotificationSettings("new_flow", {
                          sound: !config.new_flow.sound,
                        })
                      }
                      className={cn(
                        "relative inline-flex h-4 w-7 items-center rounded-full transition-colors",
                        config.new_flow.sound ? "bg-blue-500" : "bg-muted",
                      )}
                    >
                      <span
                        className={cn(
                          "inline-block h-2 w-2 transform rounded-full bg-white transition-transform",
                          config.new_flow.sound
                            ? "translate-x-4"
                            : "translate-x-1",
                        )}
                      />
                    </button>
                  </div>
                </div>
              )}
            </div>

            {/* 错误通知 */}
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <span className="text-sm">请求失败</span>
                <button
                  onClick={() =>
                    updateNotificationSettings("error_flow", {
                      enabled: !config.error_flow.enabled,
                    })
                  }
                  className={cn(
                    "relative inline-flex h-5 w-9 items-center rounded-full transition-colors",
                    config.error_flow.enabled ? "bg-blue-500" : "bg-muted",
                  )}
                >
                  <span
                    className={cn(
                      "inline-block h-3 w-3 transform rounded-full bg-white transition-transform",
                      config.error_flow.enabled
                        ? "translate-x-5"
                        : "translate-x-1",
                    )}
                  />
                </button>
              </div>
              {config.error_flow.enabled && (
                <div className="ml-4 space-y-1">
                  <div className="flex items-center justify-between text-xs">
                    <span>桌面通知</span>
                    <button
                      onClick={() =>
                        updateNotificationSettings("error_flow", {
                          desktop: !config.error_flow.desktop,
                        })
                      }
                      className={cn(
                        "relative inline-flex h-4 w-7 items-center rounded-full transition-colors",
                        config.error_flow.desktop ? "bg-blue-500" : "bg-muted",
                      )}
                    >
                      <span
                        className={cn(
                          "inline-block h-2 w-2 transform rounded-full bg-white transition-transform",
                          config.error_flow.desktop
                            ? "translate-x-4"
                            : "translate-x-1",
                        )}
                      />
                    </button>
                  </div>
                  <div className="flex items-center justify-between text-xs">
                    <span>声音提示</span>
                    <button
                      onClick={() =>
                        updateNotificationSettings("error_flow", {
                          sound: !config.error_flow.sound,
                        })
                      }
                      className={cn(
                        "relative inline-flex h-4 w-7 items-center rounded-full transition-colors",
                        config.error_flow.sound ? "bg-blue-500" : "bg-muted",
                      )}
                    >
                      <span
                        className={cn(
                          "inline-block h-2 w-2 transform rounded-full bg-white transition-transform",
                          config.error_flow.sound
                            ? "translate-x-4"
                            : "translate-x-1",
                        )}
                      />
                    </button>
                  </div>
                </div>
              )}
            </div>

            {/* 阈值警告通知 */}
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <span className="text-sm">阈值警告</span>
                <button
                  onClick={() =>
                    updateNotificationSettings("latency_warning", {
                      enabled: !config.latency_warning.enabled,
                    })
                  }
                  className={cn(
                    "relative inline-flex h-5 w-9 items-center rounded-full transition-colors",
                    config.latency_warning.enabled ? "bg-blue-500" : "bg-muted",
                  )}
                >
                  <span
                    className={cn(
                      "inline-block h-3 w-3 transform rounded-full bg-white transition-transform",
                      config.latency_warning.enabled
                        ? "translate-x-5"
                        : "translate-x-1",
                    )}
                  />
                </button>
              </div>
              {config.latency_warning.enabled && (
                <div className="ml-4 space-y-1">
                  <div className="flex items-center justify-between text-xs">
                    <span>桌面通知</span>
                    <button
                      onClick={() =>
                        updateNotificationSettings("latency_warning", {
                          desktop: !config.latency_warning.desktop,
                        })
                      }
                      className={cn(
                        "relative inline-flex h-4 w-7 items-center rounded-full transition-colors",
                        config.latency_warning.desktop
                          ? "bg-blue-500"
                          : "bg-muted",
                      )}
                    >
                      <span
                        className={cn(
                          "inline-block h-2 w-2 transform rounded-full bg-white transition-transform",
                          config.latency_warning.desktop
                            ? "translate-x-4"
                            : "translate-x-1",
                        )}
                      />
                    </button>
                  </div>
                  <div className="flex items-center justify-between text-xs">
                    <span>声音提示</span>
                    <button
                      onClick={() =>
                        updateNotificationSettings("latency_warning", {
                          sound: !config.latency_warning.sound,
                        })
                      }
                      className={cn(
                        "relative inline-flex h-4 w-7 items-center rounded-full transition-colors",
                        config.latency_warning.sound
                          ? "bg-blue-500"
                          : "bg-muted",
                      )}
                    >
                      <span
                        className={cn(
                          "inline-block h-2 w-2 transform rounded-full bg-white transition-transform",
                          config.latency_warning.sound
                            ? "translate-x-4"
                            : "translate-x-1",
                        )}
                      />
                    </button>
                  </div>
                </div>
              )}
            </div>

            {/* Token 警告通知 */}
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <span className="text-sm">Token 警告</span>
                <button
                  onClick={() =>
                    updateNotificationSettings("token_warning", {
                      enabled: !config.token_warning.enabled,
                    })
                  }
                  className={cn(
                    "relative inline-flex h-5 w-9 items-center rounded-full transition-colors",
                    config.token_warning.enabled ? "bg-blue-500" : "bg-muted",
                  )}
                >
                  <span
                    className={cn(
                      "inline-block h-3 w-3 transform rounded-full bg-white transition-transform",
                      config.token_warning.enabled
                        ? "translate-x-5"
                        : "translate-x-1",
                    )}
                  />
                </button>
              </div>
              {config.token_warning.enabled && (
                <div className="ml-4 space-y-1">
                  <div className="flex items-center justify-between text-xs">
                    <span>桌面通知</span>
                    <button
                      onClick={() =>
                        updateNotificationSettings("token_warning", {
                          desktop: !config.token_warning.desktop,
                        })
                      }
                      className={cn(
                        "relative inline-flex h-4 w-7 items-center rounded-full transition-colors",
                        config.token_warning.desktop
                          ? "bg-blue-500"
                          : "bg-muted",
                      )}
                    >
                      <span
                        className={cn(
                          "inline-block h-2 w-2 transform rounded-full bg-white transition-transform",
                          config.token_warning.desktop
                            ? "translate-x-4"
                            : "translate-x-1",
                        )}
                      />
                    </button>
                  </div>
                  <div className="flex items-center justify-between text-xs">
                    <span>声音提示</span>
                    <button
                      onClick={() =>
                        updateNotificationSettings("token_warning", {
                          sound: !config.token_warning.sound,
                        })
                      }
                      className={cn(
                        "relative inline-flex h-4 w-7 items-center rounded-full transition-colors",
                        config.token_warning.sound ? "bg-blue-500" : "bg-muted",
                      )}
                    >
                      <span
                        className={cn(
                          "inline-block h-2 w-2 transform rounded-full bg-white transition-transform",
                          config.token_warning.sound
                            ? "translate-x-4"
                            : "translate-x-1",
                        )}
                      />
                    </button>
                  </div>
                </div>
              )}
            </div>
          </div>

          {/* 测试按钮 */}
          <div className="pt-2">
            <button
              onClick={testNotification}
              disabled={!config.enabled || permissionStatus !== "granted"}
              className="w-full px-3 py-2 text-sm bg-muted hover:bg-muted/80 rounded disabled:opacity-50 disabled:cursor-not-allowed"
            >
              发送测试通知
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

export default NotificationSettings;
