import { create } from 'zustand'
import { createConnectionSlice, ConnectionSlice } from './connection'
import { createUiSlice, UiSlice } from './ui'
import { createModelSlice, ModelSlice } from './model'
import { createAgentSlice, AgentSlice } from './agent'
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
import { createPluginSlice, PluginSlice } from './plugin'
import { createCronSlice, CronSlice } from './cron'

export type AppStore = ConnectionSlice &
  UiSlice &
  ModelSlice &
  AgentSlice &
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
  PluginSlice &
  CronSlice

export const useStore = create<AppStore>()((...a) => ({
  ...createConnectionSlice(...a),
  ...createUiSlice(...a),
  ...createModelSlice(...a),
  ...createAgentSlice(...a),
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
  ...createPluginSlice(...a),
  ...createCronSlice(...a),
}))
