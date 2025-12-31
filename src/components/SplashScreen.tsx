/**
 * 启动画面组件
 *
 * 应用启动时显示 Logo 动画，然后淡出进入主界面
 */

import { useState, useEffect } from "react";
import styled, { keyframes } from "styled-components";

const fadeIn = keyframes`
  from { opacity: 0; transform: scale(0.9); }
  to { opacity: 1; transform: scale(1); }
`;

const fadeOut = keyframes`
  from { opacity: 1; }
  to { opacity: 0; }
`;

const pulse = keyframes`
  0%, 100% { opacity: 1; }
  50% { opacity: 0.5; }
`;

const Container = styled.div<{ $isExiting: boolean }>`
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  background: linear-gradient(
    135deg,
    hsl(var(--background)) 0%,
    hsl(var(--muted)) 100%
  );
  z-index: 9999;
  animation: ${({ $isExiting }) => ($isExiting ? fadeOut : fadeIn)} 0.5s
    ease-out forwards;
`;

const LogoContainer = styled.div`
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 24px;
  animation: ${fadeIn} 0.8s ease-out;
`;

const Logo = styled.img`
  width: 120px;
  height: 120px;
  object-fit: contain;
  filter: drop-shadow(0 20px 40px rgba(0, 0, 0, 0.15));
`;

const AppName = styled.h1`
  font-size: 32px;
  font-weight: 700;
  color: hsl(var(--foreground));
  margin: 0;
`;

const LoadingText = styled.p`
  font-size: 14px;
  color: hsl(var(--muted-foreground));
  margin: 0;
  animation: ${pulse} 1.5s ease-in-out infinite;
`;

interface SplashScreenProps {
  onComplete: () => void;
  duration?: number;
}

export function SplashScreen({
  onComplete,
  duration = 1500,
}: SplashScreenProps) {
  const [isExiting, setIsExiting] = useState(false);

  useEffect(() => {
    const exitTimer = setTimeout(() => {
      setIsExiting(true);
    }, duration);

    const completeTimer = setTimeout(() => {
      onComplete();
    }, duration + 500);

    return () => {
      clearTimeout(exitTimer);
      clearTimeout(completeTimer);
    };
  }, [duration, onComplete]);

  return (
    <Container $isExiting={isExiting}>
      <LogoContainer>
        <Logo src="/logo.png" alt="ProxyCast" />
        <AppName>ProxyCast</AppName>
        <LoadingText>正在加载...</LoadingText>
      </LogoContainer>
    </Container>
  );
}
