import React, { useState } from "react";
import {
  HelpCircle,
  X,
  Search,
  Filter,
  Zap,
  Hash,
  Tag,
  ChevronDown,
  ChevronRight,
  Copy,
  Check,
  FileText,
} from "lucide-react";
import { cn } from "@/lib/utils";

interface FilterHelpProps {
  onClose?: () => void;
  onInsertExample?: (example: string) => void;
  className?: string;
}

// ============================================================================
// 过滤器分类
// ============================================================================

interface FilterCategory {
  name: string;
  icon: React.ReactNode;
  description: string;
  filters: FilterInfo[];
}

interface FilterInfo {
  syntax: string;
  description: string;
  example?: string;
  hasArg: boolean;
}

const FILTER_CATEGORIES: FilterCategory[] = [
  {
    name: "基础过滤器",
    icon: <Filter className="h-4 w-4" />,
    description: "按模型、提供商、状态等基本属性过滤",
    filters: [
      {
        syntax: "~m <pattern>",
        description: "模型名称匹配（支持 * 通配符）",
        example: "~m claude*",
        hasArg: true,
      },
      {
        syntax: "~p <provider>",
        description: "提供商匹配",
        example: "~p kiro",
        hasArg: true,
      },
      {
        syntax: "~s <state>",
        description: "状态匹配 (pending/streaming/completed/failed/cancelled)",
        example: "~s completed",
        hasArg: true,
      },
    ],
  },
  {
    name: "特性过滤器",
    icon: <Zap className="h-4 w-4" />,
    description: "按 Flow 特性过滤",
    filters: [
      { syntax: "~e", description: "有错误", example: "~e", hasArg: false },
      { syntax: "~t", description: "有工具调用", example: "~t", hasArg: false },
      { syntax: "~k", description: "有思维链", example: "~k", hasArg: false },
      {
        syntax: "~starred",
        description: "已收藏",
        example: "~starred",
        hasArg: false,
      },
    ],
  },
  {
    name: "标签过滤器",
    icon: <Tag className="h-4 w-4" />,
    description: "按标签过滤",
    filters: [
      {
        syntax: "~tag <name>",
        description: "包含指定标签",
        example: "~tag important",
        hasArg: true,
      },
    ],
  },
  {
    name: "内容搜索",
    icon: <Search className="h-4 w-4" />,
    description: "搜索请求或响应内容",
    filters: [
      {
        syntax: "~b <regex>",
        description: "请求或响应内容匹配（正则表达式）",
        example: '~b "hello"',
        hasArg: true,
      },
      {
        syntax: "~bq <regex>",
        description: "仅请求内容匹配",
        example: "~bq user",
        hasArg: true,
      },
      {
        syntax: "~bs <regex>",
        description: "仅响应内容匹配",
        example: "~bs assistant",
        hasArg: true,
      },
    ],
  },
  {
    name: "数值比较",
    icon: <Hash className="h-4 w-4" />,
    description: "按 Token 数量或延迟过滤",
    filters: [
      {
        syntax: "~tokens <op> <n>",
        description: "Token 数量比较 (>, >=, <, <=, =)",
        example: "~tokens >1000",
        hasArg: true,
      },
      {
        syntax: "~latency <op> <n>",
        description: "延迟比较（支持 s/ms 后缀）",
        example: "~latency >5s",
        hasArg: true,
      },
    ],
  },
];

const OPERATORS = [
  {
    symbol: "&",
    description: "AND 逻辑 - 同时满足两个条件",
    example: "~p kiro & ~m claude",
  },
  {
    symbol: "|",
    description: "OR 逻辑 - 满足任一条件",
    example: "~p kiro | ~p gemini",
  },
  { symbol: "!", description: "NOT 逻辑 - 取反", example: "!~e" },
  {
    symbol: "()",
    description: "分组 - 控制优先级",
    example: "(~p kiro | ~p gemini) & ~m claude",
  },
];

const EXAMPLES = [
  { name: "Claude 模型", expr: "~m claude" },
  { name: "Kiro 提供商的 Claude 模型", expr: "~p kiro & ~m claude" },
  { name: "有错误或高延迟", expr: "~e | ~latency >5s" },
  { name: "没有错误", expr: "!~e" },
  { name: "大 Token 请求", expr: "~tokens >10000" },
  { name: "有工具调用的已完成请求", expr: "~t & ~s completed" },
  { name: "已收藏的有思维链请求", expr: "~starred & ~k" },
  {
    name: "多提供商的高 Token 请求",
    expr: "(~p kiro | ~p gemini) & ~tokens >1000",
  },
];

// ============================================================================
// 组件实现
// ============================================================================

export function FilterHelp({
  onClose,
  onInsertExample,
  className,
}: FilterHelpProps) {
  const [expandedCategories, setExpandedCategories] = useState<Set<string>>(
    new Set(FILTER_CATEGORIES.map((c) => c.name)),
  );
  const [copiedExample, setCopiedExample] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");

  const toggleCategory = (name: string) => {
    setExpandedCategories((prev) => {
      const next = new Set(prev);
      if (next.has(name)) {
        next.delete(name);
      } else {
        next.add(name);
      }
      return next;
    });
  };

  const handleCopyExample = async (example: string) => {
    try {
      await navigator.clipboard.writeText(example);
      setCopiedExample(example);
      setTimeout(() => setCopiedExample(null), 2000);
    } catch (e) {
      console.error("复制失败:", e);
    }
  };

  const handleInsertExample = (example: string) => {
    onInsertExample?.(example);
  };

  // 过滤搜索结果
  const filteredCategories = FILTER_CATEGORIES.map((category) => ({
    ...category,
    filters: category.filters.filter(
      (f) =>
        !searchQuery ||
        f.syntax.toLowerCase().includes(searchQuery.toLowerCase()) ||
        f.description.toLowerCase().includes(searchQuery.toLowerCase()),
    ),
  })).filter((c) => c.filters.length > 0);

  const filteredExamples = EXAMPLES.filter(
    (e) =>
      !searchQuery ||
      e.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
      e.expr.toLowerCase().includes(searchQuery.toLowerCase()),
  );

  return (
    <div className={cn("rounded-lg border bg-card", className)}>
      {/* 头部 */}
      <div className="flex items-center justify-between px-4 py-3 border-b">
        <div className="flex items-center gap-2">
          <HelpCircle className="h-5 w-5 text-primary" />
          <span className="font-medium">过滤表达式帮助</span>
        </div>
        {onClose && (
          <button
            onClick={onClose}
            className="p-1 hover:bg-muted rounded"
            title="关闭"
          >
            <X className="h-4 w-4 text-muted-foreground" />
          </button>
        )}
      </div>

      {/* 搜索框 */}
      <div className="px-4 py-2 border-b">
        <div className="relative">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
          <input
            type="text"
            placeholder="搜索过滤器..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="w-full rounded-lg border bg-background pl-9 pr-4 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary"
          />
        </div>
      </div>

      {/* 内容区域 */}
      <div className="max-h-[60vh] overflow-y-auto">
        {/* 过滤器分类 */}
        <div className="p-4 space-y-3">
          {filteredCategories.map((category) => (
            <div key={category.name} className="rounded-lg border">
              <button
                onClick={() => toggleCategory(category.name)}
                className="w-full flex items-center gap-2 px-3 py-2 hover:bg-muted/50 rounded-t-lg"
              >
                {expandedCategories.has(category.name) ? (
                  <ChevronDown className="h-4 w-4 text-muted-foreground" />
                ) : (
                  <ChevronRight className="h-4 w-4 text-muted-foreground" />
                )}
                {category.icon}
                <span className="font-medium text-sm">{category.name}</span>
                <span className="text-xs text-muted-foreground ml-auto">
                  {category.filters.length} 个过滤器
                </span>
              </button>

              {expandedCategories.has(category.name) && (
                <div className="px-3 pb-3 space-y-2">
                  <p className="text-xs text-muted-foreground pl-6">
                    {category.description}
                  </p>
                  <div className="space-y-1">
                    {category.filters.map((filter) => (
                      <FilterItem
                        key={filter.syntax}
                        filter={filter}
                        onCopy={handleCopyExample}
                        onInsert={handleInsertExample}
                        copied={copiedExample === filter.example}
                      />
                    ))}
                  </div>
                </div>
              )}
            </div>
          ))}
        </div>

        {/* 逻辑运算符 */}
        <div className="px-4 pb-4">
          <div className="rounded-lg border">
            <div className="flex items-center gap-2 px-3 py-2 border-b bg-muted/30">
              <Zap className="h-4 w-4 text-purple-500" />
              <span className="font-medium text-sm">逻辑运算符</span>
            </div>
            <div className="p-3 space-y-2">
              {OPERATORS.map((op) => (
                <div key={op.symbol} className="flex items-start gap-3 text-sm">
                  <code className="px-2 py-0.5 rounded bg-purple-100 dark:bg-purple-900/30 text-purple-700 dark:text-purple-300 font-mono font-bold">
                    {op.symbol}
                  </code>
                  <div className="flex-1">
                    <p className="text-muted-foreground">{op.description}</p>
                    <button
                      onClick={() => handleInsertExample(op.example)}
                      className="text-xs text-primary hover:underline font-mono mt-1"
                    >
                      {op.example}
                    </button>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>

        {/* 示例 */}
        <div className="px-4 pb-4">
          <div className="rounded-lg border">
            <div className="flex items-center gap-2 px-3 py-2 border-b bg-muted/30">
              <FileText className="h-4 w-4 text-green-500" />
              <span className="font-medium text-sm">常用示例</span>
            </div>
            <div className="p-3 space-y-2">
              {filteredExamples.map((example) => (
                <div
                  key={example.expr}
                  className="flex items-center justify-between gap-2 text-sm"
                >
                  <div className="flex-1 min-w-0">
                    <p className="text-muted-foreground truncate">
                      {example.name}
                    </p>
                    <code className="text-xs font-mono text-primary">
                      {example.expr}
                    </code>
                  </div>
                  <div className="flex items-center gap-1">
                    <button
                      onClick={() => handleCopyExample(example.expr)}
                      className="p-1 hover:bg-muted rounded"
                      title="复制"
                    >
                      {copiedExample === example.expr ? (
                        <Check className="h-3 w-3 text-green-500" />
                      ) : (
                        <Copy className="h-3 w-3 text-muted-foreground" />
                      )}
                    </button>
                    {onInsertExample && (
                      <button
                        onClick={() => handleInsertExample(example.expr)}
                        className="px-2 py-0.5 text-xs rounded bg-primary/10 text-primary hover:bg-primary/20"
                      >
                        使用
                      </button>
                    )}
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

// ============================================================================
// 子组件
// ============================================================================

interface FilterItemProps {
  filter: FilterInfo;
  onCopy: (example: string) => void;
  onInsert: (example: string) => void;
  copied: boolean;
}

function FilterItem({ filter, onCopy, onInsert, copied }: FilterItemProps) {
  return (
    <div className="flex items-start gap-2 pl-6 py-1.5 rounded hover:bg-muted/50">
      <code className="px-2 py-0.5 rounded bg-blue-100 dark:bg-blue-900/30 text-blue-700 dark:text-blue-300 font-mono text-xs whitespace-nowrap">
        {filter.syntax}
      </code>
      <div className="flex-1 min-w-0">
        <p className="text-xs text-muted-foreground">{filter.description}</p>
        {filter.example && (
          <div className="flex items-center gap-2 mt-1">
            <code className="text-xs font-mono text-green-600 dark:text-green-400">
              {filter.example}
            </code>
            <button
              onClick={() => onCopy(filter.example!)}
              className="p-0.5 hover:bg-muted rounded"
              title="复制"
            >
              {copied ? (
                <Check className="h-3 w-3 text-green-500" />
              ) : (
                <Copy className="h-3 w-3 text-muted-foreground" />
              )}
            </button>
            <button
              onClick={() => onInsert(filter.example!)}
              className="text-xs text-primary hover:underline"
            >
              使用
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

export default FilterHelp;
