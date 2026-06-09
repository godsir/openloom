import { useLocale } from '../../i18n'

export default function TypingIndicator() {
  const { t } = useLocale()
  return (
    <span className="typing-dots" aria-label={t('chat.aiReplying')}>
      <span />
      <span />
      <span />
    </span>
  )
}
