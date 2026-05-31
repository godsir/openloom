const bc = new BroadcastChannel('pet')

let timer: ReturnType<typeof setTimeout> | null = null

export function sendPetState(state: string): void {
  if (timer) clearTimeout(timer)
  timer = setTimeout(() => {
    bc.postMessage({ type: 'state', state })
  }, 100)
}
