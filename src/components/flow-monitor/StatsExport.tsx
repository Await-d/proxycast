/**
 * 统计报告导出组件
 *
 * 提供统计报告的导出功能，支持 JSON、Markdown 和 CSV 格式。
 *
 * **Validates: Requirements 9.7**
 */

import React, { useState, useCallback } from "react";
import {
  Download,
  FileJson,
  FileText,
  Table,
  Loader2,
  Check,
  AlertCircle,
} from "lucide-react";
import {
  enhancedStatsApi,
  type FlowFilter,
  type StatsTimeRange,
  type ReportFormat,
} from "@/lib/api/flowMonitor";
import { cn } from "@/lib/utils";

interface StatsExportProps {
  /** 过滤条件 */
  filter?: FlowFilter;
  /** 时间范围 */
  timeRange?: StatsTimeRange;
  /** 导出完成回调 */
  onExportComplete?: (format: ReportFormat) => void;
  /** 是否显示为按钮组 */
  buttonGroup?: boolean;
  /** 自定义类名 */
  className?: string;
}

interface ExportOption {
  format: ReportFormat;
  label: string;
  icon: React.ReactNode;
  description: string;
  extension: string;
  mimeType: string;
}

const exportOptions: ExportOption[] = [
  {
    format: "json",
    label: "JSON",
    icon: <FileJson className="h-4 w-4" />,
    description: "结构化数据格式，适合程序处理",
    extension: "json",
    mimeType: "application/json",
  },
  {
    format: "markdown",
    label: "Markdown",
    icon: <FileText className="h-4 w-4" />,
    description: "可读性强的文档格式，适合报告",
    extension: "md",
    mimeType: "text/markdown",
  },
  {
    format: "csv",
    label: "CSV",
    icon: <Table className="h-4 w-4" />,
    description: "表格格式，适合 Excel 等工具",
    extension: "csv",
    mimeType: "text/csv",
  },
];

export function StatsExport({
  filter = {},
  timeRange,
  onExportComplete,
  buttonGroup = false,
  className,
}: StatsExportProps) {
  const [exporting, setExporting] = useState<ReportFormat | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<ReportFormat | null>(null);

  const getDefaultTimeRange = useCallback((): StatsTimeRange => {
    const now = new Date();
    return {
      start: new Date(now.getTime() - 24 * 60 * 60 * 1000).toISOString(),
      end: now.toISOString(),
    };
  }, []);

  const handleExport = useCallback(
    async (option: ExportOption) => {
      try {
        setExporting(option.format);
        setError(null);
        setSuccess(null);

        const report = await enhancedStatsApi.exportStatsReport(
          filter,
          timeRange || getDefaultTimeRange(),
          option.format,
        );

        // 创建下载
        const blob = new Blob([report], { type: option.mimeType });
        const url = URL.createObjectURL(blob);
        const timestamp = new Date().toISOString().replace(/[:.]/g, "-");
        const filename = `flow_stats_${timestamp}.${option.extension}`;

        const link = document.createElement("a");
        link.href = url;
        link.download = filename;
        document.body.appendChild(link);
        link.click();
        document.body.removeChild(link);
        URL.revokeObjectURL(url);

        setSuccess(option.format);
        onExportComplete?.(option.format);

        // 3 秒后清除成功状态
        setTimeout(() => setSuccess(null), 3000);
      } catch (e) {
        console.error("Failed to export stats report:", e);
        setError(e instanceof Error ? e.message : "导出失败");
      } finally {
        setExporting(null);
      }
    },
    [filter, timeRange, getDefaultTimeRange, onExportComplete],
  );

  if (buttonGroup) {
    return (
      <div className={cn("flex items-center gap-1", className)}>
        {exportOptions.map((option) => (
          <button
            key={option.format}
            onClick={() => handleExport(option)}
            disabled={exporting !== null}
            className={cn(
              "flex items-center gap-1 rounded border px-2 py-1 text-sm hover:bg-muted disabled:opacity-50",
              success === option.format && "border-green-500 text-green-600",
            )}
            title={option.description}
          >
            {exporting === option.format ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : success === option.format ? (
              <Check className="h-3 w-3" />
            ) : (
              option.icon
            )}
            {option.label}
          </button>
        ))}
      </div>
    );
  }

  return (
    <div className={cn("rounded-lg border bg-card p-4", className)}>
      <h3 className="text-sm font-medium mb-4 flex items-center gap-2">
        <Download className="h-4 w-4" />
        导出统计报告
      </h3>

      {error && (
        <div className="mb-4 rounded-lg border border-red-200 bg-red-50 dark:bg-red-950/20 p-3">
          <div className="flex items-center gap-2 text-red-600 dark:text-red-400 text-sm">
            <AlertCircle className="h-4 w-4" />
            <span>{error}</span>
          </div>
        </div>
      )}

      <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
        {exportOptions.map((option) => (
          <button
            key={option.format}
            onClick={() => handleExport(option)}
            disabled={exporting !== null}
            className={cn(
              "flex flex-col items-center gap-2 rounded-lg border p-4 hover:bg-muted transition-colors disabled:opacity-50",
              success === option.format &&
                "border-green-500 bg-green-50 dark:bg-green-950/20",
            )}
          >
            <div
              className={cn(
                "p-2 rounded-full",
                success === option.format
                  ? "bg-green-100 dark:bg-green-900/30"
                  : "bg-muted",
              )}
            >
              {exporting === option.format ? (
                <Loader2 className="h-5 w-5 animate-spin" />
              ) : success === option.format ? (
                <Check className="h-5 w-5 text-green-600" />
              ) : (
                <span className="text-muted-foreground">{option.icon}</span>
              )}
            </div>
            <div className="text-sm font-medium">{option.label}</div>
            <div className="text-xs text-muted-foreground text-center">
              {option.description}
            </div>
          </button>
        ))}
      </div>

      <div className="mt-4 text-xs text-muted-foreground">
        导出的报告将包含当前时间范围内的所有统计数据，包括请求趋势、Token
        分布、延迟直方图等。
      </div>
    </div>
  );
}

/**
 * 统计导出下拉菜单组件
 *
 * 提供一个下拉菜单形式的导出选项。
 */
interface StatsExportDropdownProps {
  /** 过滤条件 */
  filter?: FlowFilter;
  /** 时间范围 */
  timeRange?: StatsTimeRange;
  /** 导出完成回调 */
  onExportComplete?: (format: ReportFormat) => void;
  /** 触发按钮内容 */
  trigger?: React.ReactNode;
  /** 自定义类名 */
  className?: string;
}

export function StatsExportDropdown({
  filter = {},
  timeRange,
  onExportComplete,
  trigger,
  className,
}: StatsExportDropdownProps) {
  const [open, setOpen] = useState(false);
  const [exporting, setExporting] = useState<ReportFormat | null>(null);

  const getDefaultTimeRange = useCallback((): StatsTimeRange => {
    const now = new Date();
    return {
      start: new Date(now.getTime() - 24 * 60 * 60 * 1000).toISOString(),
      end: now.toISOString(),
    };
  }, []);

  const handleExport = useCallback(
    async (option: ExportOption) => {
      try {
        setExporting(option.format);

        const report = await enhancedStatsApi.exportStatsReport(
          filter,
          timeRange || getDefaultTimeRange(),
          option.format,
        );

        // 创建下载
        const blob = new Blob([report], { type: option.mimeType });
        const url = URL.createObjectURL(blob);
        const timestamp = new Date().toISOString().replace(/[:.]/g, "-");
        const filename = `flow_stats_${timestamp}.${option.extension}`;

        const link = document.createElement("a");
        link.href = url;
        link.download = filename;
        document.body.appendChild(link);
        link.click();
        document.body.removeChild(link);
        URL.revokeObjectURL(url);

        onExportComplete?.(option.format);
        setOpen(false);
      } catch (e) {
        console.error("Failed to export stats report:", e);
      } finally {
        setExporting(null);
      }
    },
    [filter, timeRange, getDefaultTimeRange, onExportComplete],
  );

  return (
    <div className={cn("relative", className)}>
      <button
        onClick={() => setOpen(!open)}
        className="flex items-center gap-1 rounded border px-2 py-1 text-sm hover:bg-muted"
      >
        {trigger || (
          <>
            <Download className="h-3 w-3" />
            导出报告
          </>
        )}
      </button>

      {open && (
        <>
          {/* 背景遮罩 */}
          <div className="fixed inset-0 z-40" onClick={() => setOpen(false)} />

          {/* 下拉菜单 */}
          <div className="absolute right-0 top-full mt-1 z-50 w-48 rounded-lg border bg-popover shadow-lg">
            <div className="p-1">
              {exportOptions.map((option) => (
                <button
                  key={option.format}
                  onClick={() => handleExport(option)}
                  disabled={exporting !== null}
                  className="flex items-center gap-2 w-full rounded px-3 py-2 text-sm hover:bg-muted disabled:opacity-50"
                >
                  {exporting === option.format ? (
                    <Loader2 className="h-4 w-4 animate-spin" />
                  ) : (
                    option.icon
                  )}
                  <span>{option.label}</span>
                </button>
              ))}
            </div>
          </div>
        </>
      )}
    </div>
  );
}

export default StatsExport;
