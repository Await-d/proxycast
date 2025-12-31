import styled from "styled-components";

// --- InputbarCore Styles ---

export const DragHandle = styled.div`
  position: absolute;
  top: -3px;
  left: 0;
  right: 0;
  height: 6px;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: row-resize;
  color: var(--muted-foreground);
  opacity: 0;
  transition: opacity 0.2s;
  z-index: 10;

  &:hover {
    opacity: 1;
  }
`;

export const Container = styled.div`
  display: flex;
  flex-direction: column;
  position: relative;
  z-index: 2;
  padding: 0 18px 18px 18px; /* Cherry Studio Exact: 0 18px 18px 18px */
  width: 100%;
  max-width: 900px;
  margin: 0 auto;
`;

export const InputBarContainer = styled.div`
  border: 1px solid hsl(var(--border));
  transition: all 0.2s ease;
  position: relative;
  border-radius: 17px;
  padding-top: 8px;
  background-color: #f4f4f5; /* Zinc-100: Distinct Gray Background */

  /* Dark mode adjustment */
  @media (prefers-color-scheme: dark) {
    background-color: #27272a; /* Zinc-800 */
    border-color: #3f3f46; /* Zinc-700 */
  }

  /* Focus state */
  &:focus-within {
    border-color: hsl(var(--primary));
    background-color: hsl(var(--background));
    box-shadow: 0 0 0 1px hsl(var(--primary));
  }

  &.file-dragging {
    border: 2px dashed #2ecc71;
    background-color: rgba(46, 204, 113, 0.03);
  }
`;

export const StyledTextarea = styled.textarea`
  padding: 0 15px;
  padding-top: 2px;
  border-radius: 0;
  display: flex;
  resize: none !important;
  overflow: auto;
  width: 100%;
  box-sizing: border-box;
  background: transparent;
  border: none;
  outline: none;
  line-height: 1.5;
  font-family: inherit;
  font-size: 14px;
  color: hsl(var(--foreground));
  min-height: 30px;

  &::placeholder {
    color: hsl(var(--muted-foreground));
  }

  &::-webkit-scrollbar {
    width: 3px;
  }
  &::-webkit-scrollbar-thumb {
    background-color: hsl(var(--border));
    border-radius: 2px;
  }
`;

export const BottomBar = styled.div`
  display: flex;
  flex-direction: row;
  justify-content: space-between;
  align-items: center;
  padding: 5px 8px;
  height: 40px;
  gap: 16px;
  position: relative;
  z-index: 2;
  flex-shrink: 0;
`;
// ... (LeftSection and RightSection seem fine without vars, skipping for brevity of replace block if possible but might as well include to be safe or target specific chunks)

// I will split this into chunks to be safe and precise.

export const LeftSection = styled.div`
  display: flex;
  align-items: center;
  flex: 1;
  min-width: 0;
  /* Cherry Studio uses ToolWrapper with margin-right: 6px, handled in component or here */
`;

export const RightSection = styled.div`
  display: flex;
  align-items: center;
  gap: 6px; /* Cherry Studio Exact: 6px */
`;

// --- InputbarTools Styles ---

export const ToolButton = styled.button`
  display: flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  border-radius: 50%;
  color: hsl(var(--muted-foreground));
  transition: all 0.2s ease-in-out;
  background: transparent;
  border: none;
  cursor: pointer;
  padding: 0;
  margin-right: 2px;

  &:hover {
    color: hsl(var(--foreground));
    background-color: hsl(var(--secondary));
  }

  &.active {
    color: hsl(var(--primary));
  }

  svg {
    width: 16px;
    height: 16px;
  }
`;

export const Divider = styled.div`
  width: 1px;
  height: 16px;
  background-color: hsl(var(--border));
  margin: 0 4px;
`;

export const SendButton = styled.button`
  display: flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  border-radius: 50%;
  background-color: transparent;
  color: hsl(var(--primary));
  border: none;
  cursor: pointer;
  transition: all 0.2s;

  &:hover:not(:disabled) {
    background-color: hsl(var(--primary-foreground));
    transform: scale(1.05);
  }

  &:disabled {
    cursor: default;
    color: hsl(var(--muted-foreground));
    opacity: 0.5;
  }
`;

// --- Image Preview Styles ---

export const ImagePreviewContainer = styled.div`
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  padding: 8px 15px;
  border-bottom: 1px solid hsl(var(--border));
`;

export const ImagePreviewItem = styled.div`
  position: relative;
  width: 60px;
  height: 60px;
  border-radius: 8px;
  overflow: hidden;
  border: 1px solid hsl(var(--border));
  background-color: hsl(var(--muted));
`;

export const ImagePreviewImg = styled.img`
  width: 100%;
  height: 100%;
  object-fit: cover;
`;

export const ImageRemoveButton = styled.button`
  position: absolute;
  top: 2px;
  right: 2px;
  width: 18px;
  height: 18px;
  border-radius: 50%;
  background-color: rgba(0, 0, 0, 0.6);
  color: white;
  border: none;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  transition: background-color 0.2s;

  &:hover {
    background-color: rgba(220, 38, 38, 0.9);
  }
`;
