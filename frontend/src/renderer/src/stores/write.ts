// frontend/src/renderer/src/stores/write.ts
import { create } from 'zustand';
import { persist } from 'zustand/middleware';

// ============================================================
// 共享类型
// ============================================================

export interface WorkspaceEntry {
  name: string;
  path: string;
  kind: 'file' | 'directory';
  extension?: string;
  children?: WorkspaceEntry[];
}

export interface WriteEditorSelectionState {
  text: string;
  from: number;
  to: number;
  lineFrom: number;
  lineTo: number;
  blockType: string | null;
  containsImage: boolean;
}

export interface QuotedSelection {
  id: string;
  text: string;
  filePath: string;
  lineFrom: number;
  lineTo: number;
  timestamp: number;
}

export interface RecentEdit {
  instruction: string;
  originalText: string;
  editedText: string;
  filePath: string;
  timestamp: number;
}

export interface DiffChunk {
  id: string;
  originalText: string;
  modifiedText: string;
  fromA: number;
  toA: number;
  fromB: number;
  toB: number;
  accepted: boolean | null;
}

export type WritePreviewMode = 'rich' | 'source' | 'live' | 'split' | 'preview';
export type WriteSaveStatus = 'saved' | 'dirty' | 'saving' | 'error';
export type WriteModalState = 'none' | 'newFile' | 'newFolder' | 'rename' | 'delete' | 'export';
export type WriteFileKind = 'text' | 'image' | 'pdf';

// ============================================================
// Slice 1: writeSettingsSlice
// ============================================================

interface WriteSettingsSlice {
  workspaceRoot: string | null;
  defaultWorkspaceRoot: string | null;
  previewMode: WritePreviewMode;
  fontSize: number;
  lineHeight: number;
  fontFamily: string;
  fileSidebarOpen: boolean;
  inlineCompletionEnabled: boolean;
  inlineCompletionModel: string | null;
  shortDebounceMs: number;
  longDebounceMs: number;
  minAcceptScore: number;
  shortMaxTokens: number;
  longMaxTokens: number;
  retrievalEnabled: boolean;
  imageStoragePath: string;
  autoSaveIntervalMs: number;

  setWorkspaceRoot: (root: string | null) => void;
  setPreviewMode: (mode: WritePreviewMode) => void;
  setFontSize: (size: number) => void;
  setLineHeight: (lh: number) => void;
  setFontFamily: (family: string) => void;
  toggleFileSidebar: () => void;
  setInlineCompletionEnabled: (enabled: boolean) => void;
  setRetrievalEnabled: (enabled: boolean) => void;
}

const createWriteSettingsSlice = (set: any, _get: any): WriteSettingsSlice => ({
  workspaceRoot: null,
  defaultWorkspaceRoot: null,
  previewMode: 'source',
  fontSize: 14,
  lineHeight: 1.8,
  fontFamily: 'system',
  fileSidebarOpen: true,
  inlineCompletionEnabled: true,
  inlineCompletionModel: null,
  shortDebounceMs: 300,
  longDebounceMs: 1500,
  minAcceptScore: 0.6,
  shortMaxTokens: 64,
  longMaxTokens: 256,
  retrievalEnabled: true,
  imageStoragePath: '.assets/',
  autoSaveIntervalMs: 900,

  setWorkspaceRoot: (root) => {
    set({ workspaceRoot: root, entriesByDir: {}, expandedDirs: {}, activeFilePath: null, fileContent: '' });
    if (root) { try { localStorage.setItem('loom:writeWorkspace', root); } catch {} }
  },
  setPreviewMode: (mode) => {
    set({ previewMode: mode });
    try { localStorage.setItem('loom:writePreviewMode', mode); } catch {}
  },
  setFontSize: (fontSize) => set({ fontSize }),
  setLineHeight: (lineHeight) => set({ lineHeight }),
  setFontFamily: (fontFamily) => set({ fontFamily }),
  toggleFileSidebar: () => set((s: any) => ({ fileSidebarOpen: !s.fileSidebarOpen })),
  setInlineCompletionEnabled: (inlineCompletionEnabled) => set({ inlineCompletionEnabled }),
  setRetrievalEnabled: (retrievalEnabled) => set({ retrievalEnabled }),
});

// ============================================================
// Slice 2: writeFilesSlice
// ============================================================

interface WriteFilesSlice {
  entriesByDir: Record<string, WorkspaceEntry[]>;
  expandedDirs: Record<string, boolean>;
  activeFilePath: string | null;
  activeFileKind: WriteFileKind;
  fileContent: string;
  saveStatus: WriteSaveStatus;
  fileLoading: boolean;
  fileError: string | null;
  fileSize: number;
  fileTruncated: boolean;

  setEntriesByDir: (dir: string, entries: WorkspaceEntry[]) => void;
  toggleDir: (dirPath: string) => void;
  setActiveFile: (path: string | null, kind: WriteFileKind) => void;
  setFileContent: (content: string) => void;
  setSaveStatus: (status: WriteSaveStatus) => void;
  setFileLoading: (loading: boolean) => void;
  setFileError: (error: string | null) => void;
  setFileSize: (size: number) => void;
  setFileTruncated: (truncated: boolean) => void;
  clearActiveFile: () => void;
  refreshTrigger: number;
  triggerRefresh: () => void;
}

const createWriteFilesSlice = (set: any, _get: any): WriteFilesSlice => ({
  entriesByDir: {},
  expandedDirs: {},
  activeFilePath: null,
  activeFileKind: 'text',
  fileContent: '',
  saveStatus: 'saved',
  fileLoading: false,
  fileError: null,
  fileSize: 0,
  fileTruncated: false,
  refreshTrigger: 0,

  setEntriesByDir: (dir, entries) =>
    set((s: any) => ({ entriesByDir: { ...s.entriesByDir, [dir]: entries } })),
  toggleDir: (dirPath) =>
    set((s: any) => ({
      expandedDirs: { ...s.expandedDirs, [dirPath]: !s.expandedDirs[dirPath] },
    })),
  setActiveFile: (path, kind) => set({ activeFilePath: path, activeFileKind: kind, fileError: null }),
  setFileContent: (content) => set({ fileContent: content }),
  setSaveStatus: (saveStatus) => set({ saveStatus }),
  setFileLoading: (fileLoading) => set({ fileLoading }),
  setFileError: (fileError) => set({ fileError }),
  setFileSize: (fileSize) => set({ fileSize }),
  setFileTruncated: (fileTruncated) => set({ fileTruncated }),
  clearActiveFile: () =>
    set({
      activeFilePath: null,
      activeFileKind: 'text',
      fileContent: '',
      saveStatus: 'saved',
      fileLoading: false,
      fileError: null,
      fileSize: 0,
      fileTruncated: false,
    }),
  triggerRefresh: () => set((s: any) => ({ refreshTrigger: s.refreshTrigger + 1 })),
});

// ============================================================
// Slice 3: writeUiSlice
// ============================================================

interface WriteUiSlice {
  assistantOpen: boolean;
  inlineAgentVisible: boolean;
  inlineAgentPosition: { x: number; y: number; placement: 'above' | 'below' };
  modalState: WriteModalState;
  modalTarget: WorkspaceEntry | null;
  toastMessage: { type: 'success' | 'error' | 'info'; text: string } | null;

  toggleAssistant: () => void;
  setAssistantOpen: (open: boolean) => void;
  setInlineAgentVisible: (visible: boolean) => void;
  setInlineAgentPosition: (pos: { x: number; y: number; placement: 'above' | 'below' }) => void;
  setModalState: (state: WriteModalState, target?: WorkspaceEntry | null) => void;
  showToast: (type: 'success' | 'error' | 'info', text: string) => void;
  clearToast: () => void;
}

const createWriteUiSlice = (set: any, _get: any): WriteUiSlice => ({
  assistantOpen: true,
  inlineAgentVisible: false,
  inlineAgentPosition: { x: 0, y: 0, placement: 'above' },
  modalState: 'none',
  modalTarget: null,
  toastMessage: null,

  toggleAssistant: () => set((s: any) => ({ assistantOpen: !s.assistantOpen })),
  setAssistantOpen: (assistantOpen) => set({ assistantOpen }),
  setInlineAgentVisible: (inlineAgentVisible) => set({ inlineAgentVisible }),
  setInlineAgentPosition: (inlineAgentPosition) => set({ inlineAgentPosition }),
  setModalState: (modalState, modalTarget = null) => set({ modalState, modalTarget }),
  showToast: (type, text) => set({ toastMessage: { type, text } }),
  clearToast: () => set({ toastMessage: null }),
});

// ============================================================
// Slice 4: writeAiSlice
// ============================================================

interface WriteAiSlice {
  selection: WriteEditorSelectionState | null;
  quotedSelections: QuotedSelection[];
  recentEdits: RecentEdit[];
  pendingAgentReview: DiffChunk[] | null;
  reviewActive: boolean;
  agentPresetId: string | null;
  fileThreads: Record<string, string>;

  setSelection: (sel: WriteEditorSelectionState | null) => void;
  addQuotedSelection: (qs: QuotedSelection) => void;
  removeQuotedSelection: (id: string) => void;
  clearQuotedSelections: () => void;
  addRecentEdit: (edit: RecentEdit) => void;
  setPendingAgentReview: (chunks: DiffChunk[] | null) => void;
  setReviewActive: (active: boolean) => void;
  acceptDiffChunk: (id: string) => void;
  rejectDiffChunk: (id: string) => void;
  setAgentPresetId: (id: string | null) => void;
  setFileThread: (filePath: string, threadId: string) => void;
  removeFileThread: (filePath: string) => void;
}

const createWriteAiSlice = (set: any, _get: any): WriteAiSlice => ({
  selection: null,
  quotedSelections: [],
  recentEdits: [],
  pendingAgentReview: null,
  reviewActive: false,
  agentPresetId: null,
  fileThreads: {},

  setSelection: (selection) => set({ selection }),
  addQuotedSelection: (qs) =>
    set((s: any) => ({ quotedSelections: [...s.quotedSelections, qs] })),
  removeQuotedSelection: (id) =>
    set((s: any) => ({
      quotedSelections: s.quotedSelections.filter((q: QuotedSelection) => q.id !== id),
    })),
  clearQuotedSelections: () => set({ quotedSelections: [] }),
  addRecentEdit: (edit) =>
    set((s: any) => ({
      recentEdits: [edit, ...s.recentEdits].slice(0, 48),
    })),
  setPendingAgentReview: (pendingAgentReview) => set({ pendingAgentReview }),
  setReviewActive: (reviewActive) => set({ reviewActive }),
  acceptDiffChunk: (id) =>
    set((s: any) => ({
      pendingAgentReview: s.pendingAgentReview?.map((c: DiffChunk) =>
        c.id === id ? { ...c, accepted: true } : c
      ) ?? null,
    })),
  rejectDiffChunk: (id) =>
    set((s: any) => ({
      pendingAgentReview: s.pendingAgentReview?.map((c: DiffChunk) =>
        c.id === id ? { ...c, accepted: false } : c
      ) ?? null,
    })),
  setAgentPresetId: (agentPresetId) => set({ agentPresetId }),
  setFileThread: (filePath, threadId) =>
    set((s: any) => ({ fileThreads: { ...s.fileThreads, [filePath]: threadId } })),
  removeFileThread: (filePath) =>
    set((s: any) => {
      const next = { ...s.fileThreads };
      delete next[filePath];
      return { fileThreads: next };
    }),
});

// ============================================================
// 组合 Store
// ============================================================

export type WriteStore = WriteSettingsSlice & WriteFilesSlice & WriteUiSlice & WriteAiSlice;

export const useWriteStore = create<WriteStore>()(
  persist(
    (set, get, _api) => ({
      ...createWriteSettingsSlice(set, get),
      ...createWriteFilesSlice(set, get),
      ...createWriteUiSlice(set, get),
      ...createWriteAiSlice(set, get),
    }),
    {
      name: 'loom:writeStore',
      partialize: (state) => ({
        workspaceRoot: state.workspaceRoot,
        defaultWorkspaceRoot: state.defaultWorkspaceRoot,
        previewMode: state.previewMode,
        fontSize: state.fontSize,
        lineHeight: state.lineHeight,
        fontFamily: state.fontFamily,
        fileSidebarOpen: state.fileSidebarOpen,
        inlineCompletionEnabled: state.inlineCompletionEnabled,
        inlineCompletionModel: state.inlineCompletionModel,
        shortDebounceMs: state.shortDebounceMs,
        longDebounceMs: state.longDebounceMs,
        minAcceptScore: state.minAcceptScore,
        shortMaxTokens: state.shortMaxTokens,
        longMaxTokens: state.longMaxTokens,
        retrievalEnabled: state.retrievalEnabled,
        imageStoragePath: state.imageStoragePath,
        autoSaveIntervalMs: state.autoSaveIntervalMs,
        agentPresetId: state.agentPresetId,
        fileThreads: state.fileThreads,
      }),
    }
  )
);
