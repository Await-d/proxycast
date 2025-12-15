import { Check, Edit2, Trash2, Zap } from "lucide-react";
import { Provider } from "@/lib/api/switch";
import { cn } from "@/lib/utils";

interface ProviderCardProps {
  provider: Provider;
  isCurrent: boolean;
  onSwitch: () => void;
  onEdit: () => void;
  onDelete: () => void;
}

export function ProviderCard({
  provider,
  isCurrent,
  onSwitch,
  onEdit,
  onDelete,
}: ProviderCardProps) {
  return (
    <div
      className={cn(
        "relative rounded-lg border p-4 transition-all",
        isCurrent
          ? "border-primary bg-primary/5 ring-1 ring-primary"
          : "hover:border-muted-foreground/50",
      )}
    >
      {isCurrent && (
        <div className="absolute -top-2 -right-2 rounded-full bg-primary p-1">
          <Check className="h-3 w-3 text-primary-foreground" />
        </div>
      )}

      <div className="flex items-start justify-between">
        <div className="flex items-center gap-2">
          <div
            className="h-8 w-8 rounded-lg flex items-center justify-center text-lg"
            style={{ backgroundColor: provider.icon_color || "#6366f1" }}
          >
            {provider.icon || provider.name.charAt(0).toUpperCase()}
          </div>
          <div>
            <h3 className="font-medium">{provider.name}</h3>
            {provider.category && (
              <span className="text-xs text-muted-foreground">
                {provider.category}
              </span>
            )}
          </div>
        </div>

        <div className="flex gap-1">
          <button
            onClick={onEdit}
            className="p-1.5 rounded hover:bg-muted"
            title="编辑"
          >
            <Edit2 className="h-3.5 w-3.5" />
          </button>
          <button
            onClick={(e) => {
              e.stopPropagation();
              if (!isCurrent) {
                onDelete();
              }
            }}
            disabled={isCurrent}
            className={cn(
              "p-1.5 rounded text-destructive",
              isCurrent
                ? "opacity-30 cursor-not-allowed"
                : "hover:bg-destructive/10",
            )}
            title={isCurrent ? "无法删除当前使用中的配置" : "删除"}
          >
            <Trash2 className="h-3.5 w-3.5" />
          </button>
        </div>
      </div>

      {provider.category === "custom" && provider.name === "ProxyCast" && (
        <p className="mt-2 text-xs text-blue-600 dark:text-blue-400">
          本地代理服务，将凭证池中的凭证转换为标准 API
        </p>
      )}
      {provider.notes && (
        <p className="mt-2 text-sm text-muted-foreground line-clamp-2">
          {provider.notes}
        </p>
      )}

      <div className="mt-4">
        {isCurrent ? (
          <span className="text-sm text-primary font-medium">当前使用中</span>
        ) : (
          <button
            onClick={onSwitch}
            className="flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground"
          >
            <Zap className="h-3.5 w-3.5" />
            切换到此配置
          </button>
        )}
      </div>
    </div>
  );
}
