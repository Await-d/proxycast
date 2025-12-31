import React, { useState, useRef, useEffect } from "react";
import { Button } from "@/components/ui/button";
import { Loader2, Plus, Languages, ArrowUp, X } from "lucide-react";
import { cn } from "@/lib/utils";
import { toast } from "sonner";
import {
  InputSection,
  InputContainer,
  CustomTextarea,
  InputToolbar,
  ToolbarGroup,
} from "../styles";
import { MessageImage } from "../types";

interface InputAreaProps {
  onSendMessage: (content: string, images: MessageImage[]) => void;
  isSending: boolean;
  disabled: boolean;
}

export const InputArea: React.FC<InputAreaProps> = ({
  onSendMessage,
  isSending,
  disabled,
}) => {
  const [inputMessage, setInputMessage] = useState("");
  const [inputFocused, setInputFocused] = useState(false);
  const [pendingImages, setPendingImages] = useState<MessageImage[]>([]);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Auto-resize textarea
  useEffect(() => {
    if (textareaRef.current) {
      textareaRef.current.style.height = "auto";
      textareaRef.current.style.height = `${textareaRef.current.scrollHeight}px`;
    }
  }, [inputMessage]);

  const handleSend = () => {
    if (!inputMessage.trim() && pendingImages.length === 0) return;
    onSendMessage(inputMessage, pendingImages);
    setInputMessage("");
    setPendingImages([]);
    // Reset height
    if (textareaRef.current) {
      textareaRef.current.style.height = "auto";
    }
  };

  const handleKeyPress = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  const handlePaste = async (e: React.ClipboardEvent) => {
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
              { data: base64Data, mediaType: item.type },
            ]);
            toast.success("图片已添加");
          };
          reader.readAsDataURL(file);
        }
        break;
      }
    }
  };

  const removeImage = (index: number) => {
    setPendingImages((prev) => prev.filter((_, i) => i !== index));
  };

  return (
    <InputSection>
      <InputContainer $focused={inputFocused}>
        {pendingImages.length > 0 && (
          <div className="flex flex-wrap gap-2 px-3 pt-2">
            {pendingImages.map((img, i) => (
              <div key={i} className="relative group">
                <img
                  src={`data:${img.mediaType};base64,${img.data}`}
                  alt="preview"
                  className="h-16 w-16 object-cover rounded-md border border-border"
                />
                <button
                  onClick={() => removeImage(i)}
                  className="absolute -top-2 -right-2 bg-destructive text-destructive-foreground rounded-full p-0.5 opacity-0 group-hover:opacity-100 transition-opacity"
                >
                  <X size={12} />
                </button>
              </div>
            ))}
          </div>
        )}

        <CustomTextarea
          ref={textareaRef}
          placeholder={
            disabled ? "请先创建会话..." : "发送消息... (@提到模型, / 命令)"
          }
          value={inputMessage}
          onChange={(e) => setInputMessage(e.target.value)}
          onKeyDown={handleKeyPress}
          onPaste={handlePaste}
          onFocus={() => setInputFocused(true)}
          onBlur={() => setInputFocused(false)}
          rows={1}
          disabled={disabled}
        />

        <InputToolbar>
          <ToolbarGroup>
            <Button
              variant="ghost"
              size="icon"
              className="h-8 w-8 text-muted-foreground hover:text-foreground"
              disabled={disabled}
            >
              <Plus size={18} />
            </Button>
            <Button
              variant="ghost"
              size="icon"
              className="h-8 w-8 text-muted-foreground hover:text-foreground"
              disabled={disabled}
            >
              <Languages size={18} />
            </Button>
          </ToolbarGroup>

          <ToolbarGroup>
            <span className="text-xs text-muted-foreground mr-2 self-center hidden sm:inline-block">
              Enter 发送
            </span>
            <Button
              size="icon"
              className={cn(
                "h-8 w-8 rounded-full transition-all duration-200",
                inputMessage.trim() || pendingImages.length > 0
                  ? "bg-primary text-primary-foreground"
                  : "bg-muted text-muted-foreground",
              )}
              onClick={handleSend}
              disabled={
                disabled ||
                isSending ||
                (!inputMessage.trim() && pendingImages.length === 0)
              }
            >
              {isSending ? (
                <Loader2 size={16} className="animate-spin" />
              ) : (
                <ArrowUp size={16} />
              )}
            </Button>
          </ToolbarGroup>
        </InputToolbar>
      </InputContainer>
    </InputSection>
  );
};
