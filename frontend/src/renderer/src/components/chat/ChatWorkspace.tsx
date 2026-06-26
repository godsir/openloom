import { useStore } from '../../stores'
import { useIMStore } from '../../stores/im'
import { useLocale } from '../../i18n'
import ChatArea from './ChatArea'
import InputArea from '../input/InputArea'
import ImSessionNotice from './ImSessionNotice'
import WelcomeScreen from '../shared/WelcomeScreen'

export default function ChatWorkspace() {
  const sessionId = useStore(s => s.currentSessionId)
  const imSource = useIMStore(s => s.imSessionSources)[sessionId || '']
  const { t } = useLocale()

  return (
    <div className="flex flex-col h-full relative">
      {sessionId ? <ChatArea /> : <WelcomeScreen />}
      {imSource
        ? <ImSessionNotice platform={imSource.platform} t={t} />
        : <InputArea />
      }
    </div>
  )
}
