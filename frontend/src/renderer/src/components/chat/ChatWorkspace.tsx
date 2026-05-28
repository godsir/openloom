import { useStore } from '../../stores'
import ChatArea from './ChatArea'
import InputArea from '../input/InputArea'
import WelcomeScreen from '../shared/WelcomeScreen'

export default function ChatWorkspace() {
  const sessionId = useStore(s => s.currentSessionId)

  return (
    <div className="flex flex-col h-full relative">
      {sessionId ? <ChatArea /> : <WelcomeScreen />}
      <InputArea />
    </div>
  )
}
