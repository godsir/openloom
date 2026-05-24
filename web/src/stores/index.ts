import { create } from 'zustand';
import { createConnectionSlice, type ConnectionSlice } from './connection-slice';
import { createSessionSlice, type SessionSlice } from './session-slice';
import { createStreamingSlice, type StreamingSlice } from './streaming-slice';
import { createUiSlice, type UiSlice } from './ui-slice';
import { createAgentSlice, type AgentSlice } from './agent-slice';
import { createModelSlice, type ModelSlice } from './model-slice';
import { createInputSlice, type InputSlice } from './input-slice';
import { createChatSlice, type ChatSlice } from './chat-slice';
import { createToastSlice, type ToastSlice } from './toast-slice';
import { createBrowserSlice, type BrowserSlice } from './browser-slice';
import { createContextSlice, type ContextSlice } from './context-slice';
import { createSelectionSlice, type SelectionSlice } from './selection-slice';
import { createPreviewSlice, type PreviewSlice } from './preview-slice';

export type StoreState = ConnectionSlice &
  SessionSlice &
  StreamingSlice &
  UiSlice &
  AgentSlice &
  ModelSlice &
  InputSlice &
  ChatSlice &
  ToastSlice &
  BrowserSlice &
  ContextSlice &
  SelectionSlice &
  PreviewSlice;

export const useStore = create<StoreState>()((set, _get, _api) => ({
  ...createConnectionSlice(set, _get),
  ...createSessionSlice(set),
  ...createStreamingSlice(set, _get),
  ...createUiSlice(set),
  ...createAgentSlice(set),
  ...createModelSlice(set),
  ...createInputSlice(set),
  ...createChatSlice(set, _get),
  ...createToastSlice(set, _get),
  ...createBrowserSlice(),
  ...createContextSlice(set),
  ...createSelectionSlice(set),
  ...createPreviewSlice(),
}));
