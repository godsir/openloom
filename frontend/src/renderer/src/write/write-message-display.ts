import { parseWritePromptForDisplay } from './quoted-selection'

export function getWriteMessageDisplayText(source: string): string {
  return parseWritePromptForDisplay(source).userInput
}

