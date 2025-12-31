/**
 * 全局应用侧边栏
 *
 * 类似 cherry-studio 的图标导航栏，始终显示在应用左侧
 */

import { useState, useEffect } from "react";
import styled from "styled-components";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import {
  Bot,
  Globe,
  Database,
  FileCode,
  Activity,
  Wrench,
  Puzzle,
  Settings,
  Moon,
  Sun,
} from "lucide-react";

type Page =
  | "provider-pool"
  | "config-management"
  | "api-server"
  | "flow-monitor"
  | "agent"
  | "tools"
  | "plugins"
  | "browser-interceptor"
  | "settings"
  | `plugin:${string}`;

interface AppSidebarProps {
  currentPage: Page;
  onNavigate: (page: Page) => void;
}

const Container = styled.div`
  display: flex;
  flex-direction: column;
  align-items: center;
  width: 54px;
  min-width: 54px;
  height: 100vh;
  padding: 12px 0;
  background-color: hsl(var(--card));
  border-right: 1px solid hsl(var(--border));
`;

const LogoContainer = styled.div`
  width: 36px;
  height: 36px;
  display: flex;
  align-items: center;
  justify-content: center;
  margin-bottom: 16px;
  cursor: pointer;
  transition: transform 0.2s;

  &:hover {
    transform: scale(1.05);
  }
`;

const LogoImg = styled.img`
  width: 32px;
  height: 32px;
  object-fit: contain;
`;

const MenusContainer = styled.div`
  display: flex;
  flex-direction: column;
  flex: 1;
  gap: 4px;
  overflow-y: auto;
  overflow-x: hidden;

  &::-webkit-scrollbar {
    display: none;
  }
`;

const BottomMenus = styled.div`
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-top: auto;
  padding-top: 8px;
  border-top: 1px solid hsl(var(--border));
`;

const IconButton = styled.button<{ $active?: boolean }>`
  width: 38px;
  height: 38px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 10px;
  border: none;
  background: ${({ $active }) =>
    $active ? "hsl(var(--primary))" : "transparent"};
  color: ${({ $active }) =>
    $active
      ? "hsl(var(--primary-foreground))"
      : "hsl(var(--muted-foreground))"};
  cursor: pointer;
  transition: all 0.2s;

  &:hover {
    background: ${({ $active }) =>
      $active ? "hsl(var(--primary))" : "hsl(var(--muted))"};
    color: ${({ $active }) =>
      $active ? "hsl(var(--primary-foreground))" : "hsl(var(--foreground))"};
  }

  svg {
    width: 20px;
    height: 20px;
  }
`;

const mainMenuItems: { id: Page; label: string; icon: typeof Bot }[] = [
  { id: "agent", label: "AI Agent", icon: Bot },
  { id: "api-server", label: "API Server", icon: Globe },
  { id: "provider-pool", label: "凭证池", icon: Database },
  { id: "config-management", label: "配置管理", icon: FileCode },
  { id: "flow-monitor", label: "Flow Monitor", icon: Activity },
  { id: "tools", label: "工具", icon: Wrench },
  { id: "plugins", label: "插件中心", icon: Puzzle },
];

export function AppSidebar({ currentPage, onNavigate }: AppSidebarProps) {
  const [theme, setTheme] = useState<"light" | "dark">(() => {
    if (typeof window !== "undefined") {
      return document.documentElement.classList.contains("dark")
        ? "dark"
        : "light";
    }
    return "light";
  });

  useEffect(() => {
    if (theme === "dark") {
      document.documentElement.classList.add("dark");
    } else {
      document.documentElement.classList.remove("dark");
    }
    localStorage.setItem("theme", theme);
  }, [theme]);

  const toggleTheme = () => {
    setTheme(theme === "dark" ? "light" : "dark");
  };

  return (
    <TooltipProvider>
      <Container>
        <Tooltip>
          <TooltipTrigger asChild>
            <LogoContainer onClick={() => onNavigate("agent")}>
              <LogoImg src="/logo.png" alt="ProxyCast" />
            </LogoContainer>
          </TooltipTrigger>
          <TooltipContent side="right">
            <span className="whitespace-nowrap">ProxyCast</span>
          </TooltipContent>
        </Tooltip>

        <MenusContainer>
          {mainMenuItems.map((item) => (
            <Tooltip key={item.id}>
              <TooltipTrigger asChild>
                <IconButton
                  $active={currentPage === item.id}
                  onClick={() => onNavigate(item.id)}
                >
                  <item.icon />
                </IconButton>
              </TooltipTrigger>
              <TooltipContent side="right">
                <span className="whitespace-nowrap">{item.label}</span>
              </TooltipContent>
            </Tooltip>
          ))}
        </MenusContainer>

        <BottomMenus>
          <Tooltip>
            <TooltipTrigger asChild>
              <IconButton onClick={toggleTheme}>
                {theme === "dark" ? <Moon /> : <Sun />}
              </IconButton>
            </TooltipTrigger>
            <TooltipContent side="right">
              <span className="whitespace-nowrap">
                {theme === "dark" ? "深色模式" : "浅色模式"}
              </span>
            </TooltipContent>
          </Tooltip>

          <Tooltip>
            <TooltipTrigger asChild>
              <IconButton
                $active={currentPage === "settings"}
                onClick={() => onNavigate("settings")}
              >
                <Settings />
              </IconButton>
            </TooltipTrigger>
            <TooltipContent side="right">
              <span className="whitespace-nowrap">设置</span>
            </TooltipContent>
          </Tooltip>
        </BottomMenus>
      </Container>
    </TooltipProvider>
  );
}
