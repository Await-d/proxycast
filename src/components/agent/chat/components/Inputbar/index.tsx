import React from "react";
import { InputbarCore } from "./components/InputbarCore";
import { toast } from "sonner";
import { useState, useCallback, useRef } from "react";
import type { MessageImage } from "../../types";

interface InputbarProps {
  input: string;
  setInput: (value: string) => void;
  onSend: (
    images?: MessageImage[],
    webSearch?: boolean,
    thinking?: boolean,
  ) => void;
  isLoading: boolean;
  disabled?: boolean;
  onClearMessages?: () => void;
}

export const Inputbar: React.FC<InputbarProps> = ({
  input,
  setInput,
  onSend,
  isLoading,
  disabled,
  onClearMessages,
}) => {
  const [activeTools, setActiveTools] = useState<Record<string, boolean>>({});
  const [pendingImages, setPendingImages] = useState<MessageImage[]>([]);
  const [isFullscreen, setIsFullscreen] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const handleToolClick = useCallback(
    (tool: string) => {
      switch (tool) {
        case "thinking":
        case "web_search":
          setActiveTools((prev) => {
            const newState = { ...prev, [tool]: !prev[tool] };
            toast.info(
              `${tool === "thinking" ? "深度思考" : "联网搜索"}${newState[tool] ? "已开启" : "已关闭"}`,
            );
            return newState;
          });
          break;
        case "clear":
          setInput("");
          setPendingImages([]);
          toast.success("已清除输入");
          break;
        case "new_topic":
          onClearMessages?.();
          setInput("");
          setPendingImages([]);
          break;
        case "attach":
          fileInputRef.current?.click();
          break;
        case "quick_action":
          toast.info("快捷指令功能开发中...");
          break;
        case "translate":
          toast.info("翻译功能开发中...");
          break;
        case "fullscreen":
          setIsFullscreen((prev) => !prev);
          toast.info(isFullscreen ? "已退出全屏" : "已进入全屏编辑");
          break;
        default:
          break;
      }
    },
    [setInput, onClearMessages, isFullscreen],
  );

  const handleFileSelect = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const files = e.target.files;
      if (!files || files.length === 0) return;

      Array.from(files).forEach((file) => {
        if (file.type.startsWith("image/")) {
          const reader = new FileReader();
          reader.onload = (event) => {
            const base64 = event.target?.result as string;
            const base64Data = base64.split(",")[1];
            setPendingImages((prev) => [
              ...prev,
              {
                data: base64Data,
                mediaType: file.type,
              },
            ]);
            toast.success(`已添加图片: ${file.name}`);
          };
          reader.readAsDataURL(file);
        } else {
          toast.info(`暂不支持该文件类型: ${file.type}`);
        }
      });

      e.target.value = "";
    },
    [],
  );

  const handlePaste = useCallback((e: React.ClipboardEvent) => {
    const items = e.clipboardData?.items;
    if (!items) return;

    for (const item of items) {
      if (item.type.startsWith("image/")) {
        e.preventDefault();
        const file = item.getAsFile();
        if (file) {
          const reader = new FileReader();
          reader.onload = (event) => {
            const base64 = event.target?.result as string;
            const base64Data = base64.split(",")[1];
            setPendingImages((prev) => [
              ...prev,
              {
                data: base64Data,
                mediaType: item.type,
              },
            ]);
            toast.success("已粘贴图片");
          };
          reader.readAsDataURL(file);
        }
        break;
      }
    }
  }, []);

  // 文件拖拽处理
  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
  }, []);

  const handleDrop = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();

    const files = e.dataTransfer.files;
    if (!files || files.length === 0) return;

    Array.from(files).forEach((file) => {
      if (file.type.startsWith("image/")) {
        const reader = new FileReader();
        reader.onload = (event) => {
          const base64 = event.target?.result as string;
          const base64Data = base64.split(",")[1];
          setPendingImages((prev) => [
            ...prev,
            {
              data: base64Data,
              mediaType: file.type,
            },
          ]);
          toast.success(`已添加图片: ${file.name}`);
        };
        reader.readAsDataURL(file);
      } else {
        toast.info(`暂不支持该文件类型: ${file.type}`);
      }
    });
  }, []);

  const handleRemoveImage = useCallback((index: number) => {
    setPendingImages((prev) => prev.filter((_, i) => i !== index));
  }, []);

  const handleSend = useCallback(() => {
    if (!input.trim() && pendingImages.length === 0) return;
    const webSearch = activeTools["web_search"] || false;
    const thinking = activeTools["thinking"] || false;
    onSend(
      pendingImages.length > 0 ? pendingImages : undefined,
      webSearch,
      thinking,
    );
    setPendingImages([]);
  }, [input, pendingImages, onSend, activeTools]);

  return (
    <div
      onDragOver={handleDragOver}
      onDrop={handleDrop}
      className={
        isFullscreen ? "fixed inset-0 z-50 bg-background p-4 flex flex-col" : ""
      }
    >
      <input
        ref={fileInputRef}
        type="file"
        accept="image/*"
        multiple
        style={{ display: "none" }}
        onChange={handleFileSelect}
      />
      <InputbarCore
        text={input}
        setText={setInput}
        onSend={handleSend}
        isLoading={isLoading}
        disabled={disabled}
        onToolClick={handleToolClick}
        activeTools={activeTools}
        pendingImages={pendingImages}
        onRemoveImage={handleRemoveImage}
        onPaste={handlePaste}
        isFullscreen={isFullscreen}
      />
    </div>
  );
};
