// Quick actions for the selection toolbar and assistant panel
// Predefined AI instructions that users can trigger with one click

export interface QuickAction {
  id: string;
  label: string;
  prompt: string;
  mode: 'chat' | 'inline'; // chat = send to assistant, inline = edit in place
}

export const DEFAULT_QUICK_ACTIONS: QuickAction[] = [
  {
    id: 'polish',
    label: '润色',
    prompt: '请润色以下文字，使其更流畅自然，保持原意不变。',
    mode: 'inline',
  },
  {
    id: 'translate',
    label: '翻译成英文',
    prompt: '请将以下文字翻译成地道的英文。',
    mode: 'inline',
  },
  {
    id: 'expand',
    label: '扩写到500字',
    prompt: '请将以下内容扩写到约500字，丰富细节但保持结构。',
    mode: 'inline',
  },
  {
    id: 'summarize',
    label: '总结要点',
    prompt: '请用简洁的语言总结以下内容的要点，用列表形式呈现。',
    mode: 'chat',
  },
  {
    id: 'formal',
    label: '改为正式语气',
    prompt: '请将以下文字改写为更正式、专业的语气。',
    mode: 'inline',
  },
  {
    id: 'explain',
    label: '解释',
    prompt: '请解释以下内容的含义和背景。',
    mode: 'chat',
  },
  {
    id: 'grammar',
    label: '修正语法',
    prompt: '请修正以下文字中的语法和拼写错误，保持原意。',
    mode: 'inline',
  },
  {
    id: 'shorter',
    label: '精简',
    prompt: '请精简以下文字，删除冗余但不丢失关键信息。',
    mode: 'inline',
  },
];

export function getQuickActionById(id: string): QuickAction | undefined {
  return DEFAULT_QUICK_ACTIONS.find((a) => a.id === id);
}
