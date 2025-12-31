/**
 * 应用主入口组件
 *
 * 管理页面路由和全局状态
 * 支持静态页面和动态插件页面路由
 * 包含启动画面和全局图标侧边栏
 *
 * _需求: 2.2, 3.2_
 */

import { useState, useEffect, useCallback } from "react";
import styled from "styled-components";
import { SplashScreen } from "./components/SplashScreen";
import { AppSidebar } from "./components/AppSidebar";
import { SettingsPage } from "./components/settings";
import { ApiServerPage } from "./components/api-server/ApiServerPage";
import { ProviderPoolPage } from "./components/provider-pool";
import { ConfigManagementPage } from "./components/config/ConfigManagementPage";
import { FlowMonitorPage } from "./pages";
import { ToolsPage } from "./components/tools/ToolsPage";
import { BrowserInterceptorTool } from "./components/tools/browser-interceptor/BrowserInterceptorTool";
import { AgentChatPage } from "./components/agent";
import { PluginUIRenderer } from "./components/plugins/PluginUIRenderer";
import { PluginsPage } from "./components/plugins/PluginsPage";
import { Toaster } from "./components/ui/sonner";
import { flowEventManager } from "./lib/flowEventManager";

/**
 * 页面类型定义
 *
 * 支持静态页面和动态插件页面
 * - 静态页面: 预定义的页面标识符
 * - 动态插件页面: `plugin:${string}` 格式，如 "plugin:machine-id-tool"
 *
 * _需求: 2.2, 3.2_
 */
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

const AppContainer = styled.div`
  display: flex;
  height: 100vh;
  width: 100vw;
  background-color: hsl(var(--background));
  overflow: hidden;
`;

const MainContent = styled.main`
  flex: 1;
  overflow: auto;
  display: flex;
  flex-direction: column;
`;

const PageWrapper = styled.div`
  flex: 1;
  padding: 24px;
  overflow: auto;
`;

function App() {
  const [showSplash, setShowSplash] = useState(true);
  const [currentPage, setCurrentPage] = useState<Page>("agent");

  // 在应用启动时初始化 Flow 事件订阅
  useEffect(() => {
    flowEventManager.subscribe();
  }, []);

  // 页面切换时重置滚动位置
  useEffect(() => {
    const mainElement = document.querySelector("main");
    if (mainElement) {
      mainElement.scrollTop = 0;
    }
  }, [currentPage]);

  const handleSplashComplete = useCallback(() => {
    setShowSplash(false);
  }, []);

  /**
   * 渲染当前页面
   *
   * 根据 currentPage 状态渲染对应的页面组件
   * - 静态页面: 直接渲染对应组件
   * - 动态插件页面: 使用 PluginUIRenderer 渲染
   *
   * _需求: 2.2, 3.2_
   */
  const renderPage = () => {
    // 检查是否为动态插件页面 (plugin:xxx 格式)
    if (currentPage.startsWith("plugin:")) {
      const pluginId = currentPage.slice(7); // 移除 "plugin:" 前缀
      return (
        <PageWrapper>
          <PluginUIRenderer pluginId={pluginId} onNavigate={setCurrentPage} />
        </PageWrapper>
      );
    }

    // 静态页面路由
    switch (currentPage) {
      case "provider-pool":
        return (
          <PageWrapper>
            <ProviderPoolPage />
          </PageWrapper>
        );
      case "config-management":
        return (
          <PageWrapper>
            <ConfigManagementPage />
          </PageWrapper>
        );
      case "api-server":
        return (
          <PageWrapper>
            <ApiServerPage />
          </PageWrapper>
        );
      case "flow-monitor":
        return (
          <PageWrapper>
            <FlowMonitorPage />
          </PageWrapper>
        );
      case "agent":
        // Agent 页面有自己的布局，不需要 PageWrapper
        return (
          <AgentChatPage onNavigate={(page) => setCurrentPage(page as Page)} />
        );
      case "tools":
        return (
          <PageWrapper>
            <ToolsPage onNavigate={setCurrentPage} />
          </PageWrapper>
        );
      case "plugins":
        return (
          <PageWrapper>
            <PluginsPage />
          </PageWrapper>
        );
      case "browser-interceptor":
        return (
          <PageWrapper>
            <BrowserInterceptorTool onNavigate={setCurrentPage} />
          </PageWrapper>
        );
      case "settings":
        return (
          <PageWrapper>
            <SettingsPage />
          </PageWrapper>
        );
      default:
        return (
          <PageWrapper>
            <ApiServerPage />
          </PageWrapper>
        );
    }
  };

  // 显示启动画面
  if (showSplash) {
    return <SplashScreen onComplete={handleSplashComplete} />;
  }

  return (
    <AppContainer>
      <AppSidebar currentPage={currentPage} onNavigate={setCurrentPage} />
      <MainContent>{renderPage()}</MainContent>
      <Toaster />
    </AppContainer>
  );
}

export default App;
