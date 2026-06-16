// Writing agent persona presets
// Built-in + custom personas that modify how the AI assistant responds

export interface AgentPreset {
  id: string;
  name: string;
  emoji: string;
  persona: string;
  builtin: boolean;
}

export const BUILTIN_PRESETS: AgentPreset[] = [
  {
    id: 'plot-coordinator',
    name: '情节统筹',
    emoji: '📋',
    persona: '你是一位专业的小说情节统筹专家。你擅长分析故事结构、人物弧线和情节发展。你的建议侧重于整体叙事的连贯性和节奏感，帮助作者构建引人入胜的故事框架。',
    builtin: true,
  },
  {
    id: 'line-editor',
    name: '文字编辑',
    emoji: '✍️',
    persona: '你是一位细致的文字编辑。你专注于句子级别的改进：措辞选择、语法修正、句式变化和语言的流畅度。你保持作者的独特声音，同时提升文本的清晰度和优美度。',
    builtin: true,
  },
  {
    id: 'foreshadowing',
    name: '伏笔追踪',
    emoji: '🔍',
    persona: '你是一位伏笔和细节追踪专家。你擅长发现故事中的伏笔线索，提醒作者注意前后矛盾，并建议如何巧妙地埋下和回收伏笔，让故事更加精妙。',
    builtin: true,
  },
  {
    id: 'continuity',
    name: '连贯性检查',
    emoji: '✅',
    persona: '你是一位文本连贯性检查员。你专注于发现时间线不一致、人物设定矛盾、地点描述冲突等问题。你的目标是确保文本内部逻辑完全自洽。',
    builtin: true,
  },
];

export function resolveAgentPreset(
  presetId: string | null,
  customPresets?: AgentPreset[],
): AgentPreset | null {
  if (!presetId) return null;

  // Check builtins
  const builtin = BUILTIN_PRESETS.find((p) => p.id === presetId);
  if (builtin) return builtin;

  // Check custom
  if (customPresets) {
    const custom = customPresets.find((p) => p.id === presetId);
    if (custom) return custom;
  }

  return null;
}

export function getAllPresets(customPresets?: AgentPreset[]): AgentPreset[] {
  return [...BUILTIN_PRESETS, ...(customPresets ?? [])];
}
