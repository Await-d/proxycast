import React, { useRef, useEffect } from "react";
import {
  Container,
  InputBarContainer,
  StyledTextarea,
  BottomBar,
  LeftSection,
  RightSection,
  SendButton,
  DragHandle,
  ImagePreviewContainer,
  ImagePreviewItem,
  ImagePreviewImg,
  ImageRemoveButton,
  ToolButton,
} from "../styles";
import { InputbarTools } from "./InputbarTools";
import { ArrowUp, Loader2, X, Languages } from "lucide-react";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import type { MessageImage } from "../../../types";

interface InputbarCoreProps {
  text: string;
  setText: (text: string) => void;
  onSend: () => void;
  isLoading?: boolean;
  disabled?: boolean;
  activeTools: Record<string, boolean>;
  onToolClick: (tool: string) => void;
  pendingImages?: MessageImage[];
  onRemoveImage?: (index: number) => void;
  onPaste?: (e: React.ClipboardEvent) => void;
  isFullscreen?: boolean;
}

export const InputbarCore: React.FC<InputbarCoreProps> = ({
  text,
  setText,
  onSend,
  isLoading = false,
  disabled = false,
  activeTools,
  onToolClick,
  pendingImages = [],
  onRemoveImage,
  onPaste,
  isFullscreen = false,
}) => {
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const hasContent = text.trim().length > 0 || pendingImages.length > 0;

  // Auto-resize textarea
  useEffect(() => {
    if (textareaRef.current) {
      if (isFullscreen) {
        textareaRef.current.style.height = "100%";
      } else {
        textareaRef.current.style.height = "auto";
        textareaRef.current.style.height = `${Math.min(textareaRef.current.scrollHeight, 300)}px`;
      }
    }
  }, [text, isFullscreen]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      if (!hasContent || disabled || isLoading) return;
      onSend();
    }
    // ESC 退出全屏
    if (e.key === "Escape" && isFullscreen) {
      onToolClick("fullscreen");
    }
  };

  return (
    <Container className={isFullscreen ? "flex-1 flex flex-col" : ""}>
      <InputBarContainer className={isFullscreen ? "flex-1 flex flex-col" : ""}>
        {!isFullscreen && <DragHandle />}

        {pendingImages.length > 0 && (
          <ImagePreviewContainer>
            {pendingImages.map((img, index) => (
              <ImagePreviewItem key={index}>
                <ImagePreviewImg
                  src={`data:${img.mediaType};base64,${img.data}`}
                  alt={`预览 ${index + 1}`}
                />
                <ImageRemoveButton onClick={() => onRemoveImage?.(index)}>
                  <X size={12} />
                </ImageRemoveButton>
              </ImagePreviewItem>
            ))}
          </ImagePreviewContainer>
        )}

        <StyledTextarea
          ref={textareaRef}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={handleKeyDown}
          onPaste={onPaste}
          placeholder={
            isFullscreen
              ? "全屏编辑模式，按 ESC 退出，Enter 发送"
              : "在这里输入消息, 按 Enter 发送"
          }
          disabled={disabled}
          className={isFullscreen ? "flex-1 resize-none" : ""}
        />

        <BottomBar>
          <LeftSection>
            <InputbarTools
              onToolClick={onToolClick}
              activeTools={activeTools}
            />
          </LeftSection>

          <RightSection>
            <TooltipProvider>
              <Tooltip>
                <TooltipTrigger asChild>
                  <ToolButton onClick={() => onToolClick("translate")}>
                    <Languages size={18} />
                  </ToolButton>
                </TooltipTrigger>
                <TooltipContent side="top">翻译</TooltipContent>
              </Tooltip>
            </TooltipProvider>
            <SendButton
              onClick={onSend}
              disabled={!hasContent || disabled || isLoading}
            >
              {isLoading ? (
                <Loader2 size={18} className="animate-spin" />
              ) : (
                <ArrowUp size={20} strokeWidth={3} />
              )}
            </SendButton>
          </RightSection>
        </BottomBar>
      </InputBarContainer>
    </Container>
  );
};
