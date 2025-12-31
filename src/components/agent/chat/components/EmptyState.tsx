import React from "react";
import styled from "styled-components";

const Container = styled.div`
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  flex: 1;
  padding: 40px;
  color: hsl(var(--muted-foreground));
`;

const Logo = styled.img`
  width: 80px;
  height: 80px;
  margin-bottom: 24px;
`;

const Title = styled.h2`
  font-size: 20px;
  font-weight: 600;
  color: hsl(var(--foreground));
  margin: 0 0 8px 0;
`;

const Description = styled.p`
  font-size: 14px;
  color: hsl(var(--muted-foreground));
  margin: 0;
  text-align: center;
  max-width: 300px;
`;

const Tips = styled.div`
  margin-top: 32px;
  display: flex;
  flex-direction: column;
  gap: 8px;
`;

const Tip = styled.div`
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: hsl(var(--muted-foreground));

  kbd {
    padding: 2px 6px;
    border-radius: 4px;
    background-color: hsl(var(--muted));
    border: 1px solid hsl(var(--border));
    font-size: 11px;
    font-family: monospace;
  }
`;

export const EmptyState: React.FC = () => {
  return (
    <Container>
      <Logo src="/logo.png" alt="ProxyCast" />
      <Title>ProxyCast Agent</Title>
      <Description>开始一段新的对话，或从左侧选择一个话题继续</Description>
      <Tips>
        <Tip>
          <kbd>Enter</kbd> 发送消息
        </Tip>
        <Tip>
          <kbd>Shift + Enter</kbd> 换行
        </Tip>
      </Tips>
    </Container>
  );
};
