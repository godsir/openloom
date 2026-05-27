import ChatArea from './ChatArea'
import InputArea from '../input/InputArea'

export default function ChatWorkspace() {
  return (
    <div className="flex flex-col h-full relative">
      <ChatArea />
      <InputArea />
    </div>
  )
}
