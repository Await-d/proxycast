/**
 * 简单的语法高亮工具
 *
 * 提供 JSON 和代码的语法高亮功能。
 */

/**
 * JSON 语法高亮
 */
export function highlightJson(json: string): string {
  return (
    json
      // 字符串（包括键名）
      .replace(/"([^"\\]*(\\.[^"\\]*)*)"/g, (match, content) => {
        // 检查是否是键名（后面跟着冒号）
        const isKey = /^"[^"]*"\s*:/.test(
          match +
            json.slice(
              json.indexOf(match) + match.length,
              json.indexOf(match) + match.length + 1,
            ),
        );
        if (isKey) {
          return `<span class="json-key">"${content}"</span>`;
        }
        return `<span class="json-string">"${content}"</span>`;
      })
      // 数字
      .replace(/:\s*(-?\d+\.?\d*)/g, ': <span class="json-number">$1</span>')
      // 布尔值
      .replace(/:\s*(true|false)/g, ': <span class="json-boolean">$1</span>')
      // null
      .replace(/:\s*(null)/g, ': <span class="json-null">$1</span>')
  );
}

/**
 * 更精确的 JSON 语法高亮（使用正则表达式逐行处理）
 */
export function highlightJsonPrecise(json: string): string {
  const lines = json.split("\n");
  return lines
    .map((line) => {
      // 键名
      let result = line.replace(
        /^(\s*)"([^"]+)":/,
        '$1<span class="json-key">"$2"</span>:',
      );

      // 字符串值
      result = result.replace(
        /:\s*"([^"\\]*(\\.[^"\\]*)*)"/g,
        ': <span class="json-string">"$1"</span>',
      );

      // 数字
      result = result.replace(
        /:\s*(-?\d+\.?\d*)(,?\s*$)/,
        ': <span class="json-number">$1</span>$2',
      );

      // 布尔值
      result = result.replace(
        /:\s*(true|false)(,?\s*$)/,
        ': <span class="json-boolean">$1</span>$2',
      );

      // null
      result = result.replace(
        /:\s*(null)(,?\s*$)/,
        ': <span class="json-null">$1</span>$2',
      );

      return result;
    })
    .join("\n");
}

/**
 * 代码语法高亮样式
 */
export const syntaxHighlightStyles = `
  .json-key { color: #9cdcfe; }
  .json-string { color: #ce9178; }
  .json-number { color: #b5cea8; }
  .json-boolean { color: #569cd6; }
  .json-null { color: #569cd6; }
  
  .code-keyword { color: #c586c0; }
  .code-string { color: #ce9178; }
  .code-number { color: #b5cea8; }
  .code-comment { color: #6a9955; }
  .code-function { color: #dcdcaa; }
  .code-variable { color: #9cdcfe; }
  
  .search-highlight { 
    background-color: #ffff00; 
    color: #000000;
    padding: 0 2px;
    border-radius: 2px;
  }
  
  .search-highlight-current {
    background-color: #ff9632;
    color: #000000;
    padding: 0 2px;
    border-radius: 2px;
  }
`;

/**
 * 在文本中高亮搜索词
 */
export function highlightSearchTerm(
  text: string,
  searchTerm: string,
  currentIndex?: number,
): { html: string; matchCount: number; matchPositions: number[] } {
  if (!searchTerm || searchTerm.trim() === "") {
    return { html: escapeHtml(text), matchCount: 0, matchPositions: [] };
  }

  const escapedSearch = escapeRegExp(searchTerm);
  const regex = new RegExp(`(${escapedSearch})`, "gi");
  const matches: number[] = [];
  let matchIndex = 0;

  const html = escapeHtml(text).replace(regex, (match) => {
    matches.push(matchIndex);
    const className =
      matchIndex === currentIndex
        ? "search-highlight-current"
        : "search-highlight";
    matchIndex++;
    return `<span class="${className}">${match}</span>`;
  });

  return { html, matchCount: matches.length, matchPositions: matches };
}

/**
 * 转义 HTML 特殊字符
 */
export function escapeHtml(text: string): string {
  const htmlEntities: Record<string, string> = {
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  };
  return text.replace(/[&<>"']/g, (char) => htmlEntities[char]);
}

/**
 * 转义正则表达式特殊字符
 */
export function escapeRegExp(string: string): string {
  return string.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/**
 * 查找所有匹配位置
 */
export function findAllMatches(text: string, searchTerm: string): number[] {
  if (!searchTerm || searchTerm.trim() === "") {
    return [];
  }

  const positions: number[] = [];
  const lowerText = text.toLowerCase();
  const lowerSearch = searchTerm.toLowerCase();
  let pos = 0;

  while ((pos = lowerText.indexOf(lowerSearch, pos)) !== -1) {
    positions.push(pos);
    pos += 1;
  }

  return positions;
}

export default {
  highlightJson,
  highlightJsonPrecise,
  highlightSearchTerm,
  escapeHtml,
  escapeRegExp,
  findAllMatches,
  syntaxHighlightStyles,
};
