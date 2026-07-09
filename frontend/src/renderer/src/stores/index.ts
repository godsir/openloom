import { create } from 'zustand'
import { createConnectionSlice, ConnectionSlice } from './connection'
import { createUiSlice, UiSlice } from './ui'
import { createModelSlice, ModelSlice } from './model'
import { createAgentSlice, AgentSlice } from './agent'
import { createTeamSlice, TeamSlice } from './team'
import { createSessionSlice, SessionSlice } from './session'
import { createChatSlice, ChatSlice } from './chat'
import { createStreamingSlice, StreamingSlice } from './streaming'
import { createInputSlice, InputSlice } from './input'
import { createSelectionSlice, SelectionSlice } from './selection'
import { createToastSlice, ToastSlice } from './toast'
import { createConfirmSlice, ConfirmSlice } from './confirm'
import { createKgSlice, KgSlice } from './kg'
import { createLightboxSlice, LightboxSlice } from './lightbox'
import { createTokenStatsSlice, TokenStatsSlice } from './tokenStats'
import { createUpdateSlice, UpdateSlice } from './update'
import { createCronSlice, CronSlice } from './cron'
import { createPlanSlice, PlanSlice } from './plan'
import { createTodoSlice, TodoSlice } from './todo'
import { createSelectionContextSlice, SelectionContextSlice } from './selectionContext'
import { createCompletionSlice, CompletionSlice } from './completion'

export type AppStore = ConnectionSlice &
  UiSlice &
  ModelSlice &
  AgentSlice &
  TeamSlice &
  SessionSlice &
  ChatSlice &
  StreamingSlice &
  InputSlice &
  SelectionSlice &
  ToastSlice &
  ConfirmSlice &
  KgSlice &
  LightboxSlice &
  TokenStatsSlice &
  UpdateSlice &
  CronSlice &
  PlanSlice &
  TodoSlice &
  SelectionContextSlice &
  CompletionSlice

export const useStore = create<AppStore>()((...a) => ({
  ...createConnectionSlice(...a),
  ...createUiSlice(...a),
  ...createModelSlice(...a),
  ...createAgentSlice(...a),
  ...createTeamSlice(...a),
  ...createSessionSlice(...a),
  ...createChatSlice(...a),
  ...createStreamingSlice(...a),
  ...createInputSlice(...a),
  ...createSelectionSlice(...a),
  ...createToastSlice(...a),
  ...createConfirmSlice(...a),
  ...createKgSlice(...a),
  ...createLightboxSlice(...a),
  ...createTokenStatsSlice(...a),
  ...createUpdateSlice(...a),
  ...createCronSlice(...a),
  ...createPlanSlice(...a),
  ...createTodoSlice(...a),
  ...createSelectionContextSlice(...a),
  ...createCompletionSlice(...a),
}))

export { useIMStore } from './im'
