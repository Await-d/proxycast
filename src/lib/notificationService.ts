/**
 * 通知服务
 *
 * 提供桌面通知功能，用于 Flow Monitor 的实时监控增强。
 *
 * **Validates: Requirements 10.1, 10.2**
 */

/**
 * 通知权限状态类型
 */
export type NotificationPermission = "default" | "denied" | "granted";

/**
 * 通知配置
 */
export interface NotificationConfig {
  /** 是否启用通知 */
  enabled: boolean;
  /** 是否在新 Flow 时通知 */
  notifyOnNewFlow: boolean;
  /** 是否在错误 Flow 时通知 */
  notifyOnError: boolean;
  /** 是否在阈值警告时通知 */
  notifyOnThresholdWarning: boolean;
  /** 是否启用声音 */
  soundEnabled: boolean;
}

/**
 * 默认通知配置
 */
export const defaultNotificationConfig: NotificationConfig = {
  enabled: true,
  notifyOnNewFlow: false,
  notifyOnError: true,
  notifyOnThresholdWarning: true,
  soundEnabled: false,
};

/**
 * 通知类型
 */
export type NotificationType = "info" | "success" | "warning" | "error";

/**
 * 通知选项
 */
export interface NotificationOptions {
  title: string;
  body: string;
  type?: NotificationType;
  tag?: string;
  requireInteraction?: boolean;
  onClick?: () => void;
}

/**
 * 通知服务类
 */
class NotificationService {
  private config: NotificationConfig = defaultNotificationConfig;
  private permission: NotificationPermission = "default";

  constructor() {
    this.checkPermission();
    this.loadConfig();
  }

  /**
   * 检查通知权限
   */
  private async checkPermission(): Promise<void> {
    if (!("Notification" in window)) {
      console.warn("[NotificationService] 浏览器不支持通知");
      return;
    }

    this.permission = Notification.permission as NotificationPermission;

    if (this.permission === "default") {
      // 不主动请求权限，等用户启用通知时再请求
    }
  }

  /**
   * 请求通知权限
   */
  async requestPermission(): Promise<boolean> {
    if (!("Notification" in window)) {
      return false;
    }

    if (this.permission === "granted") {
      return true;
    }

    try {
      const result = await Notification.requestPermission();
      this.permission = result as NotificationPermission;
      return result === "granted";
    } catch (e) {
      console.error("[NotificationService] 请求权限失败:", e);
      return false;
    }
  }

  /**
   * 加载配置
   */
  private loadConfig(): void {
    try {
      const saved = localStorage.getItem("notification_config");
      if (saved) {
        this.config = { ...defaultNotificationConfig, ...JSON.parse(saved) };
      }
    } catch (e) {
      console.error("[NotificationService] 加载配置失败:", e);
    }
  }

  /**
   * 保存配置
   */
  private saveConfig(): void {
    try {
      localStorage.setItem("notification_config", JSON.stringify(this.config));
    } catch (e) {
      console.error("[NotificationService] 保存配置失败:", e);
    }
  }

  /**
   * 获取当前配置
   */
  getConfig(): NotificationConfig {
    return { ...this.config };
  }

  /**
   * 更新配置
   */
  updateConfig(config: Partial<NotificationConfig>): void {
    this.config = { ...this.config, ...config };
    this.saveConfig();
  }

  /**
   * 检查是否可以发送通知
   */
  canNotify(): boolean {
    return (
      this.config.enabled &&
      "Notification" in window &&
      this.permission === "granted"
    );
  }

  /**
   * 获取权限状态
   */
  getPermissionStatus(): NotificationPermission {
    return this.permission;
  }

  /**
   * 发送通知
   */
  async notify(options: NotificationOptions): Promise<void> {
    if (!this.canNotify()) {
      return;
    }

    try {
      const notification = new Notification(options.title, {
        body: options.body,
        tag: options.tag,
        requireInteraction: options.requireInteraction,
        icon: "/icon.png",
      });

      if (options.onClick) {
        notification.onclick = () => {
          window.focus();
          options.onClick?.();
          notification.close();
        };
      }

      // 播放声音
      if (this.config.soundEnabled) {
        this.playSound(options.type);
      }

      // 自动关闭
      if (!options.requireInteraction) {
        setTimeout(() => notification.close(), 5000);
      }
    } catch (e) {
      console.error("[NotificationService] 发送通知失败:", e);
    }
  }

  /**
   * 播放通知声音
   */
  private playSound(type?: NotificationType): void {
    // 使用 Web Audio API 播放简单的提示音
    try {
      const audioContext = new (
        window.AudioContext || (window as any).webkitAudioContext
      )();
      const oscillator = audioContext.createOscillator();
      const gainNode = audioContext.createGain();

      oscillator.connect(gainNode);
      gainNode.connect(audioContext.destination);

      // 根据类型设置不同的音调
      switch (type) {
        case "error":
          oscillator.frequency.value = 300;
          break;
        case "warning":
          oscillator.frequency.value = 400;
          break;
        case "success":
          oscillator.frequency.value = 600;
          break;
        default:
          oscillator.frequency.value = 500;
      }

      oscillator.type = "sine";
      gainNode.gain.value = 0.1;

      oscillator.start();
      oscillator.stop(audioContext.currentTime + 0.1);
    } catch (e) {
      console.error("[NotificationService] 播放声音失败:", e);
    }
  }

  /**
   * 新 Flow 通知
   */
  notifyNewFlow(model: string, provider: string): void {
    if (!this.config.notifyOnNewFlow) {
      return;
    }

    this.notify({
      title: "新的 LLM 请求",
      body: `${provider} - ${model}`,
      type: "info",
      tag: "new-flow",
    });
  }

  /**
   * 错误 Flow 通知
   */
  notifyError(flowId: string, errorMessage: string): void {
    if (!this.config.notifyOnError) {
      return;
    }

    this.notify({
      title: "LLM 请求失败",
      body: errorMessage,
      type: "error",
      tag: `error-${flowId}`,
      requireInteraction: true,
    });
  }

  /**
   * 阈值警告通知
   */
  notifyThresholdWarning(
    flowId: string,
    warnings: {
      latencyExceeded?: boolean;
      tokenExceeded?: boolean;
      actualLatency?: number;
      actualTokens?: number;
    },
  ): void {
    if (!this.config.notifyOnThresholdWarning) {
      return;
    }

    const messages: string[] = [];
    if (warnings.latencyExceeded && warnings.actualLatency) {
      messages.push(`延迟: ${(warnings.actualLatency / 1000).toFixed(2)}s`);
    }
    if (warnings.tokenExceeded && warnings.actualTokens) {
      messages.push(`Token: ${warnings.actualTokens}`);
    }

    if (messages.length === 0) {
      return;
    }

    this.notify({
      title: "阈值警告",
      body: messages.join(", "),
      type: "warning",
      tag: `threshold-${flowId}`,
    });
  }
}

// 导出单例
export const notificationService = new NotificationService();
export default notificationService;
