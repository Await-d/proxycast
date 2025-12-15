import { useState, useEffect, useMemo } from "react";
import {
  Plus,
  RefreshCw,
  Check,
  Trash2,
  Download,
  FileText,
} from "lucide-react";
import { AppType, Prompt } from "@/lib/api/prompts";
import { usePrompts } from "@/hooks/usePrompts";
import { cn } from "@/lib/utils";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { HelpTip } from "@/components/HelpTip";

const apps: { id: AppType; label: string; filename: string }[] = [
  { id: "claude", label: "Claude Code", filename: "CLAUDE.md" },
  { id: "codex", label: "Codex", filename: "AGENTS.md" },
  { id: "gemini", label: "Gemini", filename: "GEMINI.md" },
];

/** Toggle Switch Component */
function ToggleSwitch({
  enabled,
  onChange,
  disabled = false,
}: {
  enabled: boolean;
  onChange: (enabled: boolean) => void;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={enabled}
      disabled={disabled}
      onClick={(e) => {
        e.stopPropagation();
        onChange(!enabled);
      }}
      className={cn(
        "relative inline-flex h-5 w-9 items-center rounded-full transition-colors focus:outline-none focus:ring-2 focus:ring-primary/20",
        enabled ? "bg-primary" : "bg-muted-foreground/30",
        disabled ? "opacity-50 cursor-not-allowed" : "cursor-pointer",
      )}
    >
      <span
        className={cn(
          "inline-block h-3.5 w-3.5 transform rounded-full bg-white transition-transform",
          enabled ? "translate-x-5" : "translate-x-1",
        )}
      />
    </button>
  );
}

export function PromptsPage() {
  const [activeApp, setActiveApp] = useState<AppType>("claude");
  const {
    prompts,
    loading,
    reload,
    savePrompt,
    deletePrompt,
    toggleEnabled,
    importFromFile,
  } = usePrompts(activeApp);

  const [selectedPromptId, setSelectedPromptId] = useState<string | null>(null);
  const [isCreating, setIsCreating] = useState(false);

  // Edit form state
  const [editName, setEditName] = useState("");
  const [editDescription, setEditDescription] = useState("");
  const [editContent, setEditContent] = useState("");
  const [saving, setSaving] = useState(false);
  const [importing, setImporting] = useState(false);
  const [deleteConfirm, setDeleteConfirm] = useState<string | null>(null);

  // Convert prompts map to array
  const promptList = useMemo(() => Object.entries(prompts), [prompts]);
  const enabledPrompt = promptList.find(([_, p]) => p.enabled);
  const selectedPrompt = selectedPromptId ? prompts[selectedPromptId] : null;
  const currentApp = apps.find((a) => a.id === activeApp)!;

  // Load prompts on mount and app change
  useEffect(() => {
    reload();
  }, [reload]);

  const handleSelectPrompt = (id: string, prompt: Prompt) => {
    setSelectedPromptId(id);
    setIsCreating(false);
    setEditName(prompt.name);
    setEditDescription(prompt.description || "");
    setEditContent(prompt.content);
  };

  const handleCreateNew = () => {
    setSelectedPromptId(null);
    setIsCreating(true);
    setEditName("");
    setEditDescription("");
    setEditContent("");
  };

  const handleSave = async () => {
    if (!editName.trim() || !editContent.trim()) return;

    setSaving(true);
    try {
      const timestamp = Math.floor(Date.now() / 1000);
      const id = isCreating ? `prompt-${Date.now()}` : selectedPromptId!;
      const prompt: Prompt = {
        id,
        app_type: activeApp,
        name: editName.trim(),
        content: editContent,
        description: editDescription.trim() || undefined,
        enabled: selectedPrompt?.enabled || false,
        createdAt: selectedPrompt?.createdAt || timestamp,
        updatedAt: timestamp,
      };
      await savePrompt(id, prompt);
      setIsCreating(false);
      setSelectedPromptId(id);
    } finally {
      setSaving(false);
    }
  };

  const handleDeleteClick = (id: string) => {
    const prompt = prompts[id];
    if (prompt?.enabled) {
      alert("无法删除已启用的提示词。请先禁用它。");
      return;
    }
    setDeleteConfirm(id);
  };

  const handleDeleteConfirm = async () => {
    if (!deleteConfirm) return;
    await deletePrompt(deleteConfirm);
    if (selectedPromptId === deleteConfirm) {
      setSelectedPromptId(null);
    }
    setDeleteConfirm(null);
  };

  const handleToggle = async (id: string, enabled: boolean) => {
    try {
      await toggleEnabled(id, enabled);
    } catch (error) {
      console.error("Failed to toggle prompt:", error);
    }
  };

  const handleImport = async () => {
    setImporting(true);
    try {
      await importFromFile();
    } catch (error) {
      console.error("Failed to import:", error);
      alert(
        "导入失败：" + (error instanceof Error ? error.message : String(error)),
      );
    } finally {
      setImporting(false);
    }
  };

  // Reset selection when switching apps
  const handleAppChange = (app: AppType) => {
    setActiveApp(app);
    setSelectedPromptId(null);
    setIsCreating(false);
  };

  return (
    <div className="h-full flex flex-col">
      <div className="mb-4">
        <h2 className="text-2xl font-bold">Prompts</h2>
        <p className="text-muted-foreground">
          管理不同应用的系统提示词（{currentApp.filename}）
        </p>
      </div>

      <HelpTip title="什么是 Prompts？" variant="amber">
        <ul className="list-disc list-inside space-y-1 text-sm text-amber-700 dark:text-amber-400">
          <li>Prompts 是 AI 工具的系统提示词，定义 AI 的行为和风格</li>
          <li>
            Claude Code 使用 CLAUDE.md，Codex 使用 AGENTS.md，Gemini 使用
            GEMINI.md
          </li>
          <li>
            可创建多个提示词模板，一键切换不同场景（如代码审查、文档编写等）
          </li>
        </ul>
      </HelpTip>

      {/* App tabs */}
      <div className="flex gap-2 border-b pb-2 mb-4">
        {apps.map((app) => (
          <button
            key={app.id}
            onClick={() => handleAppChange(app.id)}
            className={cn(
              "px-4 py-2 rounded-t-lg text-sm font-medium transition-colors",
              activeApp === app.id
                ? "bg-primary text-primary-foreground"
                : "hover:bg-muted text-muted-foreground",
            )}
          >
            {app.label}
          </button>
        ))}
      </div>

      {/* Status bar */}
      <div className="mb-4 p-3 rounded-lg bg-muted/50 border text-sm text-muted-foreground">
        共 {promptList.length} 个提示词 ·{" "}
        {enabledPrompt
          ? `当前启用: ${enabledPrompt[1].name}`
          : "暂无启用的提示词"}
      </div>

      {/* Main content - left/right split */}
      <div className="flex-1 flex gap-4 min-h-0">
        {/* Left list */}
        <div className="w-80 flex flex-col border rounded-lg">
          <div className="p-3 border-b flex items-center justify-between">
            <span className="text-sm font-medium">提示词列表</span>
            <div className="flex gap-1">
              <button
                onClick={handleImport}
                disabled={importing}
                className="p-1.5 rounded hover:bg-muted"
                title={`从 ${currentApp.filename} 导入`}
              >
                <Download
                  className={cn("h-4 w-4", importing && "animate-pulse")}
                />
              </button>
              <button
                onClick={reload}
                className="p-1.5 rounded hover:bg-muted"
                title="刷新"
              >
                <RefreshCw
                  className={cn("h-4 w-4", loading && "animate-spin")}
                />
              </button>
              <button
                onClick={handleCreateNew}
                className="p-1.5 rounded hover:bg-muted text-primary"
                title="新建"
              >
                <Plus className="h-4 w-4" />
              </button>
            </div>
          </div>

          <div className="flex-1 overflow-auto p-2 space-y-1">
            {loading ? (
              <div className="flex items-center justify-center py-8">
                <RefreshCw className="h-5 w-5 animate-spin text-muted-foreground" />
              </div>
            ) : promptList.length === 0 ? (
              <div className="text-center py-8 text-muted-foreground text-sm">
                <FileText className="h-8 w-8 mx-auto mb-2 opacity-50" />
                <p>暂无提示词</p>
                <button
                  onClick={handleCreateNew}
                  className="text-primary hover:underline mt-1"
                >
                  创建第一个
                </button>
              </div>
            ) : (
              promptList.map(([id, prompt]) => (
                <div
                  key={id}
                  onClick={() => handleSelectPrompt(id, prompt)}
                  className={cn(
                    "p-3 rounded-lg cursor-pointer transition-colors relative",
                    selectedPromptId === id
                      ? "bg-primary/10 border border-primary"
                      : "hover:bg-muted border border-transparent",
                  )}
                >
                  <div className="flex items-center gap-3">
                    {/* Toggle switch */}
                    <ToggleSwitch
                      enabled={prompt.enabled}
                      onChange={(enabled) => handleToggle(id, enabled)}
                    />

                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2">
                        <span className="font-medium text-sm truncate">
                          {prompt.name}
                        </span>
                        {prompt.enabled && (
                          <span className="flex-shrink-0 w-4 h-4 rounded-full bg-primary flex items-center justify-center">
                            <Check className="h-2.5 w-2.5 text-primary-foreground" />
                          </span>
                        )}
                      </div>
                      {prompt.description && (
                        <p className="text-xs text-muted-foreground truncate mt-0.5">
                          {prompt.description}
                        </p>
                      )}
                    </div>
                  </div>
                </div>
              ))
            )}
          </div>
        </div>

        {/* Right edit panel */}
        <div className="flex-1 border rounded-lg flex flex-col min-w-0">
          {!selectedPrompt && !isCreating ? (
            <div className="flex-1 flex items-center justify-center text-muted-foreground">
              <div className="text-center">
                <FileText className="h-12 w-12 mx-auto mb-3 opacity-30" />
                <p>选择一个提示词进行编辑</p>
                <p className="text-sm mt-1">或点击 + 创建新的提示词</p>
              </div>
            </div>
          ) : (
            <>
              <div className="p-4 border-b space-y-4">
                <div className="flex items-center justify-between">
                  <h3 className="font-semibold">
                    {isCreating ? "新建提示词" : "编辑提示词"}
                  </h3>
                  {selectedPrompt && (
                    <div className="flex items-center gap-3">
                      <div className="flex items-center gap-2 text-sm">
                        <span className="text-muted-foreground">启用</span>
                        <ToggleSwitch
                          enabled={selectedPrompt.enabled}
                          onChange={(enabled) =>
                            handleToggle(selectedPromptId!, enabled)
                          }
                        />
                      </div>
                      <button
                        onClick={() => handleDeleteClick(selectedPromptId!)}
                        disabled={selectedPrompt.enabled}
                        className={cn(
                          "p-1.5 rounded",
                          selectedPrompt.enabled
                            ? "opacity-30 cursor-not-allowed"
                            : "hover:bg-destructive/10 text-destructive",
                        )}
                        title={
                          selectedPrompt.enabled
                            ? "无法删除已启用的提示词"
                            : "删除"
                        }
                      >
                        <Trash2 className="h-4 w-4" />
                      </button>
                    </div>
                  )}
                </div>

                <div className="grid grid-cols-2 gap-4">
                  <div>
                    <label className="block text-sm font-medium mb-1">
                      名称
                    </label>
                    <input
                      type="text"
                      value={editName}
                      onChange={(e) => setEditName(e.target.value)}
                      className="w-full px-3 py-2 rounded-lg border bg-background focus:ring-2 focus:ring-primary/20 focus:border-primary outline-none"
                      placeholder="提示词名称"
                    />
                  </div>
                  <div>
                    <label className="block text-sm font-medium mb-1">
                      描述
                    </label>
                    <input
                      type="text"
                      value={editDescription}
                      onChange={(e) => setEditDescription(e.target.value)}
                      className="w-full px-3 py-2 rounded-lg border bg-background focus:ring-2 focus:ring-primary/20 focus:border-primary outline-none"
                      placeholder="可选描述"
                    />
                  </div>
                </div>
              </div>

              <div className="flex-1 p-4 flex flex-col min-h-0">
                <label className="block text-sm font-medium mb-2">
                  内容{" "}
                  <span className="text-xs text-muted-foreground font-normal">
                    (启用后将同步到 {currentApp.filename})
                  </span>
                </label>
                <textarea
                  value={editContent}
                  onChange={(e) => setEditContent(e.target.value)}
                  className="flex-1 w-full px-3 py-2 rounded-lg border bg-muted/50 focus:ring-2 focus:ring-primary/20 focus:border-primary outline-none font-mono text-sm resize-none"
                  placeholder="输入系统提示词内容..."
                />
              </div>

              <div className="p-4 border-t flex justify-end gap-3">
                <button
                  onClick={() => {
                    setSelectedPromptId(null);
                    setIsCreating(false);
                  }}
                  className="px-4 py-2 rounded-lg border hover:bg-muted"
                >
                  取消
                </button>
                <button
                  onClick={handleSave}
                  disabled={saving || !editName.trim() || !editContent.trim()}
                  className="px-4 py-2 rounded-lg bg-primary text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
                >
                  {saving ? "保存中..." : "保存"}
                </button>
              </div>
            </>
          )}
        </div>
      </div>

      <ConfirmDialog
        isOpen={!!deleteConfirm}
        title="删除确认"
        message="确定要删除这个 Prompt 吗？"
        onConfirm={handleDeleteConfirm}
        onCancel={() => setDeleteConfirm(null)}
      />
    </div>
  );
}
