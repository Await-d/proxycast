import React, { useState } from "react";
import styled from "styled-components";
import {
  Settings2,
  ChevronDown,
  ChevronRight,
  HelpCircle,
  X,
} from "lucide-react";
import { Switch } from "@/components/ui/switch";
import { Slider } from "@/components/ui/slider";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";
import { Button } from "@/components/ui/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";

// --- Styled Components ---

const SettingsContainer = styled.div`
  width: 300px;
  background-color: hsl(var(--background));
  border-left: 1px solid hsl(var(--border));
  display: flex;
  flex-direction: column;
  height: 100%;
  flex-shrink: 0;
`;

const Header = styled.div`
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 14px 16px;
  border-bottom: 1px solid hsl(var(--border));

  .title {
    font-size: 14px;
    font-weight: 600;
    display: flex;
    align-items: center;
    gap: 8px;
  }
`;

const SectionContainer = styled.div`
  /* padding: 16px; removed to move padding into content */
`;

const SectionTitle = styled.div`
  font-size: 12px;
  font-weight: 500;
  color: hsl(var(--muted-foreground));
  padding: 12px 16px;
  width: 100%;
  display: flex;
  align-items: center;
  gap: 4px;
  cursor: pointer;
  transition: color 0.2s;

  &:hover {
    color: hsl(var(--foreground));
  }
`;

const SectionContent = styled(CollapsibleContent)`
  padding: 0 16px 16px 16px;
`;

const SettingRow = styled.div`
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 16px;

  &:last-child {
    margin-bottom: 0;
  }

  .label {
    font-size: 13px;
    color: hsl(var(--foreground));
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .desc {
    font-size: 11px;
    color: hsl(var(--muted-foreground));
    margin-top: 2px;
  }
`;

const HelpIcon = () => (
  <HelpCircle size={12} className="text-muted-foreground opacity-70" />
);

interface CollapsibleSectionProps {
  title: string;
  children: React.ReactNode;
  defaultOpen?: boolean;
}

const CollapsibleSection: React.FC<CollapsibleSectionProps> = ({
  title,
  children,
  defaultOpen = true,
}) => {
  const [isOpen, setIsOpen] = useState(defaultOpen);

  return (
    <Collapsible open={isOpen} onOpenChange={setIsOpen}>
      <SectionContainer>
        <CollapsibleTrigger asChild>
          <SectionTitle>
            {isOpen ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
            {title}
          </SectionTitle>
        </CollapsibleTrigger>
        <SectionContent>{children}</SectionContent>
      </SectionContainer>
    </Collapsible>
  );
};

interface ChatSettingsProps {
  onClose: () => void;
}

export const ChatSettings: React.FC<ChatSettingsProps> = ({ onClose }) => {
  // Local state for UI toggles (Mocking functional settings)
  const [fontSize, setFontSize] = useState([14]);

  return (
    <SettingsContainer>
      <Header>
        <div className="title">
          <Settings2 size={16} />
          <span>设置</span>
        </div>
        <Button
          variant="ghost"
          size="icon"
          className="h-6 w-6"
          onClick={onClose}
        >
          <X size={14} />
        </Button>
      </Header>

      <ScrollArea className="flex-1">
        {/* Message Settings */}
        <CollapsibleSection title="消息设置">
          <SettingRow>
            <div className="label">显示提示词</div>
            <Switch defaultChecked />
          </SettingRow>

          <SettingRow>
            <div className="label">使用衬线字体</div>
            <Switch />
          </SettingRow>

          <SettingRow>
            <div className="label">
              思考内容自动折叠
              <HelpIcon />
            </div>
            <Switch defaultChecked />
          </SettingRow>

          <SettingRow>
            <div className="label">显示消息大纲</div>
            <Switch />
          </SettingRow>

          <SettingRow>
            <div className="label">消息样式</div>
            <Select defaultValue="simple">
              <SelectTrigger className="w-[100px] h-7 text-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="simple">简洁</SelectItem>
                <SelectItem value="bubble">气泡</SelectItem>
              </SelectContent>
            </Select>
          </SettingRow>

          <SettingRow>
            <div className="label">多模型回答样式</div>
            <Select defaultValue="tag">
              <SelectTrigger className="w-[100px] h-7 text-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="tag">标签模式</SelectItem>
                <SelectItem value="split">分栏模式</SelectItem>
              </SelectContent>
            </Select>
          </SettingRow>

          <SettingRow>
            <div className="label">对话导航按钮</div>
            <Select defaultValue="none">
              <SelectTrigger className="w-[100px] h-7 text-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="none">不显示</SelectItem>
                <SelectItem value="show">显示</SelectItem>
              </SelectContent>
            </Select>
          </SettingRow>

          <div className="mt-4 mb-2">
            <div className="text-xs mb-2 flex justify-between">
              <span>消息字体大小</span>
              <span className="text-muted-foreground">{fontSize[0]}px</span>
            </div>
            <Slider
              value={fontSize}
              onValueChange={setFontSize}
              min={12}
              max={24}
              step={1}
              className="w-full"
            />
            <div className="flex justify-between text-[10px] text-muted-foreground mt-1">
              <span>A</span>
              <span>默认</span>
              <span>A</span>
            </div>
          </div>
        </CollapsibleSection>

        <Separator />

        {/* Math Settings */}
        <CollapsibleSection title="数学公式设置">
          <SettingRow>
            <div className="label">数学公式引擎</div>
            <Select defaultValue="katex">
              <SelectTrigger className="w-[100px] h-7 text-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="katex">KaTeX</SelectItem>
                <SelectItem value="mathjax">MathJax</SelectItem>
              </SelectContent>
            </Select>
          </SettingRow>

          <SettingRow>
            <div className="label">
              启用 $...$
              <HelpIcon />
            </div>
            <Switch defaultChecked />
          </SettingRow>
        </CollapsibleSection>

        <Separator />

        {/* Code Settings */}
        <CollapsibleSection title="代码块设置">
          <SettingRow>
            <div className="label">代码风格</div>
            <Select defaultValue="auto">
              <SelectTrigger className="w-[100px] h-7 text-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="auto">auto</SelectItem>
                <SelectItem value="dark">dark</SelectItem>
                <SelectItem value="light">light</SelectItem>
              </SelectContent>
            </Select>
          </SettingRow>

          <SettingRow>
            <div className="label">
              花式代码块
              <HelpIcon />
            </div>
            <Switch defaultChecked />
          </SettingRow>

          <SettingRow>
            <div className="label">
              代码执行
              <HelpIcon />
            </div>
            <Switch />
          </SettingRow>

          <SettingRow>
            <div className="label">代码编辑器</div>
            <Switch />
          </SettingRow>

          <SettingRow>
            <div className="label">代码显示行号</div>
            <Switch />
          </SettingRow>

          <SettingRow>
            <div className="label">代码块可折叠</div>
            <Switch />
          </SettingRow>

          <SettingRow>
            <div className="label">代码块可换行</div>
            <Switch />
          </SettingRow>

          <SettingRow>
            <div className="label">
              启用预览工具
              <HelpIcon />
            </div>
            <Switch />
          </SettingRow>
        </CollapsibleSection>

        <Separator />

        {/* Input Settings */}
        <CollapsibleSection title="输入设置">
          <SettingRow>
            <div className="label">显示预估 Token 数</div>
            <Switch />
          </SettingRow>

          <SettingRow>
            <div className="label">长文本粘贴为文件</div>
            <Switch />
          </SettingRow>

          <SettingRow>
            <div className="label">Markdown 渲染输入消息</div>
            <Switch />
          </SettingRow>

          <SettingRow>
            <div className="label">3 个空格快速翻译</div>
            <Switch />
          </SettingRow>
        </CollapsibleSection>
      </ScrollArea>
    </SettingsContainer>
  );
};
