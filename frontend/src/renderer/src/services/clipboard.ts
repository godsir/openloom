interface ClipboardBridge {
  writeText: (text: string) => Promise<void>
}

export async function copyText(
  text: string,
  bridge: ClipboardBridge = { writeText: window.loom.writeText },
): Promise<void> {
  await bridge.writeText(text)
}

