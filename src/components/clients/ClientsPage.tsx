import { useState } from "react";
import { AppType } from "@/lib/api/switch";
import { AppTabs } from "./AppTabs";
import { ProviderList } from "./ProviderList";
import { HelpTip } from "@/components/HelpTip";

export function ClientsPage() {
  const [activeApp, setActiveApp] = useState<AppType>("claude");

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-2xl font-bold">配置切换</h2>
        <p className="text-muted-foreground">
          一键切换 Claude Code / Codex / Gemini CLI 的 API 配置，快速在不同
          Provider 间切换
        </p>
      </div>

      <HelpTip title="关于 ProxyCast 本地代理" variant="blue">
        <p className="text-sm text-blue-700 dark:text-blue-400">
          添加名为 "ProxyCast" 的 Provider
          后，可将凭证池中的凭证（Kiro/Gemini/Claude 等）转换为标准
          OpenAI/Anthropic API， 供 Claude Code、Codex、Cherry Studio
          等工具使用。配置 API 地址为{" "}
          <code className="px-1 py-0.5 rounded bg-blue-100 dark:bg-blue-900">
            http://localhost:8999
          </code>
        </p>
      </HelpTip>

      <AppTabs activeApp={activeApp} onAppChange={setActiveApp} />
      <ProviderList appType={activeApp} />
    </div>
  );
}
