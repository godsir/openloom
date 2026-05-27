import { StateCreator } from 'zustand'

export type WsState = 'connected' | 'reconnecting' | 'disconnected'

export interface ConnectionSlice {
  wsState: WsState
  port: number
  reconnectAttempt: number
  setWsState: (state: WsState) => void
  setPort: (port: number) => void
  setReconnectAttempt: (n: number) => void
}

export const createConnectionSlice: StateCreator<ConnectionSlice> = (set) => ({
  wsState: 'disconnected',
  port: 0,
  reconnectAttempt: 0,
  setWsState: (wsState) => set({ wsState }),
  setPort: (port) => set({ port }),
  setReconnectAttempt: (n) => set({ reconnectAttempt: n }),
})
