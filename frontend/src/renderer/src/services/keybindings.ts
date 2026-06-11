// ── Types ──

export type ModifierKey = 'ctrl' | 'alt' | 'shift' | 'meta'

export interface Keybinding {
  /** Normalized key string (e.g. "ctrl+n") */
  keys: string
  /** Display label used in settings UI (e.g. "Ctrl+N") */
  display: string
}

export interface CommandDef {
  /** Unique command id like "nav:new-conversation" */
  id: string
  /** Human-readable label (i18n key) */
  labelKey: string
  /** Human-readable description (i18n key) */
  descKey: string
  /** Category for grouping in settings */
  category: 'navigation' | 'ui'
  /** Default keybinding */
  defaultKeys: string
  /** If true, this shortcut fires even when focus is inside input/textarea */
  allowInInput?: boolean
}

export interface ResolvedCommand extends CommandDef {
  /** Currently active keybinding (custom override or default) */
  currentKeys: string
}

// ── Normalization helpers ──

const MODIFIER_MAP: Record<string, ModifierKey> = {
  control: 'ctrl',
  alt: 'alt',
  shift: 'shift',
  meta: 'meta',
}

const SPECIAL_KEY_MAP: Record<string, string> = {
  escape: 'escape',
  tab: 'tab',
  enter: 'enter',
  backspace: 'backspace',
  delete: 'delete',
  ' ': 'space',
  arrowup: 'arrowup',
  arrowdown: 'arrowdown',
  arrowleft: 'arrowleft',
  arrowright: 'arrowright',
  home: 'home',
  end: 'end',
  pageup: 'pageup',
  pagedown: 'pagedown',
}

/**
 * Normalize a KeyboardEvent into a keybinding string like "ctrl+n".
 * Modifier order is fixed: ctrl+alt+shift+meta+key.
 */
export function eventToKeyString(e: KeyboardEvent): string {
  const parts: string[] = []
  if (e.ctrlKey) parts.push('ctrl')
  if (e.altKey) parts.push('alt')
  if (e.shiftKey) parts.push('shift')
  if (e.metaKey) parts.push('meta')

  const key = e.key.toLowerCase()

  // Skip pure modifier presses (no non-modifier key)
  if (key === 'control' || key === 'alt' || key === 'shift' || key === 'meta') {
    return ''
  }

  // Map special keys
  if (SPECIAL_KEY_MAP[key]) {
    parts.push(SPECIAL_KEY_MAP[key])
  } else {
    parts.push(key)
  }

  return parts.join('+')
}

/**
 * Convert a normalized key string ("ctrl+n") to display form ("Ctrl+N").
 */
export function keyStringToDisplay(keys: string): string {
  return keys
    .split('+')
    .map((part) => {
      if (['ctrl', 'alt', 'shift', 'meta'].includes(part)) {
        return part.charAt(0).toUpperCase() + part.slice(1)
      }
      if (part.length === 1) return part.toUpperCase()
      return part
    })
    .join('+')
}

/** Parse a stored keybinding string (may be empty for disabled). Returns normalized form. */
export function normalizeKeyString(raw: string): string {
  if (!raw || !raw.trim()) return ''
  const lower = raw.trim().toLowerCase()
  const parts = lower.split('+').map((p) => p.trim()).filter(Boolean)
  const modifiers = parts.filter((p) => ['ctrl', 'alt', 'shift', 'meta'].includes(p))
  const key = parts.filter((p) => !['ctrl', 'alt', 'shift', 'meta'].includes(p))[0] || ''
  const ordered: string[] = []
  if (modifiers.includes('ctrl')) ordered.push('ctrl')
  if (modifiers.includes('alt')) ordered.push('alt')
  if (modifiers.includes('shift')) ordered.push('shift')
  if (modifiers.includes('meta')) ordered.push('meta')
  if (key) ordered.push(key)
  return ordered.join('+')
}

/** Check if a target element is an input, textarea, contenteditable, or select */
export function isEditableTarget(target: EventTarget | null): boolean {
  if (!target || !(target instanceof HTMLElement)) return false
  const tag = target.tagName
  if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return true
  if (target.isContentEditable) return true
  if (target.getAttribute('role') === 'textbox') return true
  if (target.closest('.cm-editor') || target.closest('.cm-content')) return true
  return false
}

// ── Default commands ──

export const DEFAULT_COMMANDS: CommandDef[] = [
  // Navigation
  {
    id: 'nav:new-conversation',
    labelKey: 'keybindings.navNewConversation',
    descKey: 'keybindings.navNewConversationDesc',
    category: 'navigation',
    defaultKeys: 'ctrl+n',
  },
  {
    id: 'nav:close-conversation',
    labelKey: 'keybindings.navCloseConversation',
    descKey: 'keybindings.navCloseConversationDesc',
    category: 'navigation',
    defaultKeys: 'ctrl+w',
  },
  {
    id: 'nav:next-conversation',
    labelKey: 'keybindings.navNextConversation',
    descKey: 'keybindings.navNextConversationDesc',
    category: 'navigation',
    defaultKeys: 'ctrl+tab',
  },
  {
    id: 'nav:prev-conversation',
    labelKey: 'keybindings.navPrevConversation',
    descKey: 'keybindings.navPrevConversationDesc',
    category: 'navigation',
    defaultKeys: 'ctrl+shift+tab',
  },
  {
    id: 'nav:search-conversations',
    labelKey: 'keybindings.navSearchConversations',
    descKey: 'keybindings.navSearchConversationsDesc',
    category: 'navigation',
    defaultKeys: 'ctrl+shift+f',
  },
  {
    id: 'nav:focus-input',
    labelKey: 'keybindings.navFocusInput',
    descKey: 'keybindings.navFocusInputDesc',
    category: 'navigation',
    defaultKeys: 'escape',
    allowInInput: true,
  },

  // UI
  {
    id: 'ui:toggle-sidebar',
    labelKey: 'keybindings.uiToggleSidebar',
    descKey: 'keybindings.uiToggleSidebarDesc',
    category: 'ui',
    defaultKeys: 'ctrl+b',
  },
  {
    id: 'ui:open-settings',
    labelKey: 'keybindings.uiOpenSettings',
    descKey: 'keybindings.uiOpenSettingsDesc',
    category: 'ui',
    defaultKeys: 'ctrl+,',
  },
  {
    id: 'ui:toggle-mode',
    labelKey: 'keybindings.uiToggleMode',
    descKey: 'keybindings.uiToggleModeDesc',
    category: 'ui',
    defaultKeys: 'ctrl+shift+e',
  },
  {
    id: 'ui:inline-edit',
    labelKey: 'keybindings.uiInlineEdit',
    descKey: 'keybindings.uiInlineEditDesc',
    category: 'ui',
    defaultKeys: 'ctrl+shift+i',
  },
  {
    id: 'ui:zoom-in',
    labelKey: 'keybindings.uiZoomIn',
    descKey: 'keybindings.uiZoomInDesc',
    category: 'ui',
    defaultKeys: 'ctrl+=',
  },
  {
    id: 'ui:zoom-out',
    labelKey: 'keybindings.uiZoomOut',
    descKey: 'keybindings.uiZoomOutDesc',
    category: 'ui',
    defaultKeys: 'ctrl+-',
  },
  {
    id: 'ui:zoom-reset',
    labelKey: 'keybindings.uiZoomReset',
    descKey: 'keybindings.uiZoomResetDesc',
    category: 'ui',
    defaultKeys: 'ctrl+0',
  },
]

export type CommandCategory = 'navigation' | 'ui'

export const CATEGORY_LABEL_I18N: Record<CommandCategory, string> = {
  navigation: 'keybindings.categoryNavigation',
  ui: 'keybindings.categoryUi',
}

// ── Registry ──

export type CommandHandler = () => void

export class KeybindingRegistry {
  private commands: Map<string, ResolvedCommand> = new Map()
  private handlers: Map<string, CommandHandler> = new Map()
  private customBindings: Record<string, string> = {}

  // ── Initialization ──

  /** Call once at startup. Loads custom bindings and merges with defaults. */
  async initialize(): Promise<void> {
    // Populate with defaults first (synchronous) so dispatch works immediately,
    // before the async preference load completes.
    this.commands.clear()
    for (const def of DEFAULT_COMMANDS) {
      this.commands.set(def.id, { ...def, currentKeys: def.defaultKeys })
    }

    // Then load custom overrides asynchronously
    try {
      this.customBindings = await window.loom.getPreference<Record<string, string>>('keybindings', {})
    } catch {
      this.customBindings = {}
    }

    // Re-apply with custom bindings
    for (const def of DEFAULT_COMMANDS) {
      const custom = this.customBindings[def.id]
      if (custom !== undefined && custom !== def.defaultKeys) {
        this.commands.set(def.id, { ...def, currentKeys: custom })
      }
    }
  }

  // ── Handler registration ──

  register(commandId: string, handler: CommandHandler): void {
    this.handlers.set(commandId, handler)
  }

  unregister(commandId: string): void {
    this.handlers.delete(commandId)
  }

  // ── Dispatch ──

  dispatch(e: KeyboardEvent): boolean {
    const keyString = eventToKeyString(e)
    if (!keyString) return false

    const matched = this.findCommandByKeys(keyString)
    if (!matched) return false

    if (!matched.allowInInput && isEditableTarget(e.target)) {
      return false
    }

    const handler = this.handlers.get(matched.id)
    if (handler) {
      e.preventDefault()
      e.stopPropagation()
      handler()
      return true
    }
    return false
  }

  // ── Binding management ──

  getResolvedCommands(): ResolvedCommand[] {
    return DEFAULT_COMMANDS.map((def) => {
      const resolved = this.commands.get(def.id)
      return resolved ?? {
        ...def,
        currentKeys: this.customBindings[def.id] ?? def.defaultKeys,
      }
    })
  }

  async rebind(commandId: string, newKeys: string): Promise<string | null> {
    const normalized = normalizeKeyString(newKeys)

    if (normalized) {
      const conflict = this.findCommandByKeys(normalized)
      if (conflict && conflict.id !== commandId) {
        return conflict.id
      }
    }

    if (normalized === this.getDefault(commandId)) {
      delete this.customBindings[commandId]
    } else {
      this.customBindings[commandId] = normalized
    }

    await this.saveCustomBindings()

    const def = DEFAULT_COMMANDS.find((c) => c.id === commandId)
    if (def) {
      this.commands.set(commandId, {
        ...def,
        currentKeys: normalized || def.defaultKeys,
      })
    }

    return null
  }

  async reset(commandId: string): Promise<void> {
    delete this.customBindings[commandId]
    await this.saveCustomBindings()

    const def = DEFAULT_COMMANDS.find((c) => c.id === commandId)
    if (def) {
      this.commands.set(commandId, {
        ...def,
        currentKeys: def.defaultKeys,
      })
    }
  }

  async resetAll(): Promise<void> {
    this.customBindings = {}
    await this.saveCustomBindings()

    for (const def of DEFAULT_COMMANDS) {
      this.commands.set(def.id, { ...def, currentKeys: def.defaultKeys })
    }
  }

  // ── Internals ──

  private findCommandByKeys(keys: string): ResolvedCommand | undefined {
    for (const cmd of this.commands.values()) {
      if (cmd.currentKeys === keys) return cmd
    }
    return undefined
  }

  private getDefault(commandId: string): string {
    const def = DEFAULT_COMMANDS.find((c) => c.id === commandId)
    return def?.defaultKeys ?? ''
  }

  private async saveCustomBindings(): Promise<void> {
    await window.loom.setPreference('keybindings', { ...this.customBindings })
  }
}

/** Singleton instance */
export const keybindingRegistry = new KeybindingRegistry()
