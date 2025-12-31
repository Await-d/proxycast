/**
 * @file AgentSkillsPanel.tsx
 * @description AI Agent 页面的 Skills 展示面板组件
 * @module components/agent
 *
 * 显示已加载的 Skills 数量和名称列表，提供管理入口。
 * 实现被动式设计：Skills 自动加载，用户无需手动选择。
 */

import { Package, Settings2, Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { ChevronDown, ChevronUp } from "lucide-react";
import { useState } from "react";

interface AgentSkillsPanelProps {
  /** 已加载的 Skills 名称列表 */
  skills: string[];
  /** 是否正在加载 */
  loading: boolean;
  /** 点击"管理 Skills"按钮的回调 */
  onManageClick: () => void;
}

/**
 * AI Agent Skills 展示面板
 *
 * 功能：
 * - 显示已加载 Skills 数量
 * - 以紧凑格式显示 Skill 名称列表（用 · 分隔）
 * - 提供"管理 Skills"按钮导航到 Skills 设置页面
 * - 无 Skills 时显示提示文本和安装链接
 * - 显示使用提示
 *
 * @param skills - 已加载的 Skills 名称列表
 * @param loading - 是否正在加载
 * @param onManageClick - 点击管理按钮的回调
 */
export function AgentSkillsPanel({
  skills,
  loading,
  onManageClick,
}: AgentSkillsPanelProps) {
  const [isOpen, setIsOpen] = useState(true);

  if (loading) {
    return (
      <Card>
        <CardContent className="py-3 px-4">
          <div className="flex items-center gap-2 text-muted-foreground">
            <Loader2 className="h-4 w-4 animate-spin" />
            <span className="text-sm">加载 Skills...</span>
          </div>
        </CardContent>
      </Card>
    );
  }

  return (
    <Collapsible open={isOpen} onOpenChange={setIsOpen}>
      <Card>
        <CollapsibleTrigger asChild>
          <button className="w-full flex items-center justify-between p-3 hover:bg-muted/50 transition-colors">
            <div className="flex items-center gap-2">
              <Package className="h-4 w-4" />
              <span className="text-sm font-medium">
                📦 已加载 {skills.length} 个 Skills
              </span>
            </div>
            {isOpen ? (
              <ChevronUp className="h-4 w-4" />
            ) : (
              <ChevronDown className="h-4 w-4" />
            )}
          </button>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <CardContent className="pt-0 pb-3 px-4 space-y-3">
            {skills.length > 0 ? (
              <>
                {/* Skills 名称列表 - 紧凑格式 */}
                <div className="text-sm text-muted-foreground">
                  {skills.join(" · ")}
                </div>

                {/* 使用提示 */}
                <p className="text-xs text-muted-foreground">
                  💡 直接描述任务，Agent 会自动使用合适的 Skill
                </p>

                {/* 管理按钮 */}
                <Button
                  variant="outline"
                  size="sm"
                  onClick={onManageClick}
                  className="w-full"
                >
                  <Settings2 className="h-4 w-4 mr-2" />
                  管理 Skills
                </Button>
              </>
            ) : (
              <>
                {/* 无 Skills 提示 */}
                <p className="text-sm text-muted-foreground">
                  暂无已安装的 Skills，
                  <button
                    onClick={onManageClick}
                    className="text-primary underline hover:no-underline"
                  >
                    去安装
                  </button>
                </p>
              </>
            )}
          </CardContent>
        </CollapsibleContent>
      </Card>
    </Collapsible>
  );
}
