import React, { memo } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import rehypeKatex from "rehype-katex";
import rehypeRaw from "rehype-raw";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneDark } from "react-syntax-highlighter/dist/esm/styles/prism";
import styled from "styled-components";
import { Copy, Check } from "lucide-react";

// Custom styles for markdown content to match Cherry Studio
const MarkdownContainer = styled.div`
  font-size: 15px;
  line-height: 1.7;
  color: hsl(var(--foreground));
  overflow-wrap: break-word;

  p {
    margin-bottom: 1em;
    &:last-child {
      margin-bottom: 0;
    }
  }

  h1,
  h2,
  h3,
  h4,
  h5,
  h6 {
    font-weight: 600;
    margin-top: 24px;
    margin-bottom: 16px;
    line-height: 1.25;
  }

  h1 {
    font-size: 1.75em;
    border-bottom: 1px solid hsl(var(--border));
    padding-bottom: 0.3em;
  }
  h2 {
    font-size: 1.5em;
    border-bottom: 1px solid hsl(var(--border));
    padding-bottom: 0.3em;
  }
  h3 {
    font-size: 1.25em;
  }
  h4 {
    font-size: 1em;
  }

  ul,
  ol {
    padding-left: 20px;
    margin-bottom: 1em;
  }

  ul {
    list-style-type: disc;
  }

  ol {
    list-style-type: decimal;
  }

  li {
    margin-bottom: 0.5em;
  }

  strong {
    font-weight: 600;
  }

  em {
    font-style: italic;
  }

  hr {
    margin: 24px 0;
    border: none;
    border-top: 1px solid hsl(var(--border));
  }

  code {
    font-family:
      ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono",
      "Courier New", monospace;
    font-size: 0.9em;
    padding: 2px 4px;
    border-radius: 4px;
    background-color: hsl(var(--muted));
    color: hsl(var(--foreground));
  }

  pre {
    margin: 16px 0;
    padding: 0;
    background: transparent;
    border-radius: 8px;
    overflow: hidden;

    code {
      padding: 0;
      background: transparent;
      color: inherit;
    }
  }

  blockquote {
    border-left: 4px solid hsl(var(--primary));
    padding-left: 16px;
    margin-left: 0;
    color: hsl(var(--muted-foreground));
    font-style: italic;
  }

  table {
    border-collapse: collapse;
    width: 100%;
    margin-bottom: 1em;
  }

  th,
  td {
    border: 1px solid hsl(var(--border));
    padding: 6px 13px;
  }

  th {
    font-weight: 600;
    background-color: hsl(var(--muted));
  }

  a {
    color: hsl(var(--primary));
    text-decoration: none;
    &:hover {
      text-decoration: underline;
    }
  }

  img {
    max-width: 100%;
    border-radius: 8px;
  }
`;

const CodeBlockContainer = styled.div`
  position: relative;
  margin: 1em 0;
  border-radius: 8px;
  overflow: hidden;
  border: 1px solid hsl(var(--border));
  background-color: #282c34; // Ensure background matches theme
`;

const CodeHeader = styled.div`
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 8px 12px;
  background-color: #282c34; // Matches oneDark background
  color: #abb2bf;
  font-size: 12px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.1);
`;

const CopyButton = styled.button`
  display: flex;
  align-items: center;
  gap: 4px;
  background: none;
  border: none;
  color: inherit;
  cursor: pointer;
  padding: 4px;
  border-radius: 4px;
  transition: background 0.2s;

  &:hover {
    background: rgba(255, 255, 255, 0.1);
    color: white;
  }
`;

interface MarkdownRendererProps {
  content: string;
}

export const MarkdownRenderer: React.FC<MarkdownRendererProps> = memo(
  ({ content }) => {
    const [copied, setCopied] = React.useState<string | null>(null);

    const handleCopy = (code: string) => {
      navigator.clipboard.writeText(code);
      setCopied(code);
      setTimeout(() => setCopied(null), 2000);
    };

    return (
      <MarkdownContainer>
        <ReactMarkdown
          remarkPlugins={[remarkGfm, remarkMath]}
          rehypePlugins={[rehypeRaw, rehypeKatex]}
          components={{
            code({ inline, className, children, ...props }: any) {
              const match = /language-(\w+)/.exec(className || "");
              const codeContent = String(children).replace(/\n$/, "");
              const language = match ? match[1] : "text";

              // Inline code
              if (inline) {
                return (
                  <code className={className} {...props}>
                    {children}
                  </code>
                );
              }

              // Block code
              const isCopied = copied === codeContent;

              return (
                <CodeBlockContainer>
                  <CodeHeader>
                    <span>{language}</span>
                    <CopyButton onClick={() => handleCopy(codeContent)}>
                      {isCopied ? <Check size={14} /> : <Copy size={14} />}
                      {isCopied ? "Copied" : "Copy"}
                    </CopyButton>
                  </CodeHeader>
                  <SyntaxHighlighter
                    style={oneDark}
                    language={language}
                    PreTag="div"
                    customStyle={{
                      margin: 0,
                      padding: "16px",
                      background: "transparent",
                      fontSize: "13px",
                    }}
                    {...props}
                  >
                    {codeContent}
                  </SyntaxHighlighter>
                </CodeBlockContainer>
              );
            },
          }}
        >
          {content}
        </ReactMarkdown>
      </MarkdownContainer>
    );
  },
);

MarkdownRenderer.displayName = "MarkdownRenderer";
