import React from "react";
import {
  Paperclip,
  Lightbulb,
  Globe,
  Zap,
  Brush,
  MessageSquareDiff,
  Maximize2,
} from "lucide-react";
import { ToolButton, Divider } from "../styles";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";

interface InputbarToolsProps {
  onToolClick?: (tool: string) => void;
  activeTools?: Record<string, boolean>;
}

export const InputbarTools: React.FC<InputbarToolsProps> = ({
  onToolClick,
  activeTools = {},
}) => {
  return (
    <TooltipProvider>
      <div className="flex items-center">
        <Tooltip>
          <TooltipTrigger asChild>
            <ToolButton onClick={() => onToolClick?.("new_topic")}>
              <MessageSquareDiff />
            </ToolButton>
          </TooltipTrigger>
          <TooltipContent side="top">新建话题</TooltipContent>
        </Tooltip>

        <Tooltip>
          <TooltipTrigger asChild>
            <ToolButton onClick={() => onToolClick?.("attach")}>
              <Paperclip />
            </ToolButton>
          </TooltipTrigger>
          <TooltipContent side="top">上传文件</TooltipContent>
        </Tooltip>

        <Tooltip>
          <TooltipTrigger asChild>
            <ToolButton
              onClick={() => onToolClick?.("thinking")}
              className={activeTools["thinking"] ? "active" : ""}
            >
              <Lightbulb
                className={activeTools["thinking"] ? "text-yellow-500" : ""}
              />
            </ToolButton>
          </TooltipTrigger>
          <TooltipContent side="top">
            深度思考 {activeTools["thinking"] ? "(已开启)" : ""}
          </TooltipContent>
        </Tooltip>

        <Tooltip>
          <TooltipTrigger asChild>
            <ToolButton
              onClick={() => onToolClick?.("web_search")}
              className={activeTools["web_search"] ? "active" : ""}
            >
              <Globe
                className={activeTools["web_search"] ? "text-blue-500" : ""}
              />
            </ToolButton>
          </TooltipTrigger>
          <TooltipContent side="top">
            联网搜索 {activeTools["web_search"] ? "(已开启)" : ""}
          </TooltipContent>
        </Tooltip>

        <Divider />

        <Tooltip>
          <TooltipTrigger asChild>
            <ToolButton onClick={() => onToolClick?.("quick_action")}>
              <Zap />
            </ToolButton>
          </TooltipTrigger>
          <TooltipContent side="top">快捷指令</TooltipContent>
        </Tooltip>

        <Tooltip>
          <TooltipTrigger asChild>
            <ToolButton onClick={() => onToolClick?.("fullscreen")}>
              <Maximize2 />
            </ToolButton>
          </TooltipTrigger>
          <TooltipContent side="top">全屏编辑</TooltipContent>
        </Tooltip>

        <Tooltip>
          <TooltipTrigger asChild>
            <ToolButton onClick={() => onToolClick?.("clear")}>
              <Brush />
            </ToolButton>
          </TooltipTrigger>
          <TooltipContent side="top">清除输入</TooltipContent>
        </Tooltip>
      </div>
    </TooltipProvider>
  );
};
