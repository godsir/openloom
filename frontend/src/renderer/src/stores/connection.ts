import { StateCreator } from 'zustand'

export type WsState = 'connected' | 'reconnecting' | 'disconnected'
export type EngineState = 'running' | 'stopped' | 'starting'

export interface ConnectionSlice {
  wsState: WsState
  engineState: EngineState
  port: number
  reconnectAttempt: number
  setWsState: (state: WsState) => void
  setEngineState: (state: EngineState) => void
  setPort: (port: number) => void
  setReconnectAttempt: (n: number) => void
}

export const createConnectionSlice: StateCreator<ConnectionSlice> = (set) => ({
  wsState: 'disconnected',
  engineState: 'stopped',
  port: 0,
  reconnectAttempt: 0,
  setWsState: (wsState) => set({ wsState }),
  setEngineState: (engineState) => set({ engineState }),
  setPort: (port) => set({ port }),
  setReconnectAttempt: (n) => set({ reconnectAttempt: n }),
})
