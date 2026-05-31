type PetSize = 'small' | 'medium' | 'large'
const SIZE_MAP: Record<PetSize, number> = { small: 128, medium: 192, large: 256 }

const bc = new BroadcastChannel('pet')
let size: PetSize = 'small'

window.loom.getPreference('petSize', 'small').then((sz: string) => {
  size = sz as PetSize
})

bc.addEventListener('message', (e: MessageEvent) => {
  const d = e.data
  if (!d || d.type !== 'cmd') return

  if (d.cmd === 'toggle' && typeof d.value === 'boolean') {
    window.loom.setPreference('petEnabled', d.value)
    window.loom.togglePet(d.value)
  } else if (d.cmd === 'size' && d.value) {
    size = d.value as PetSize
    window.loom.setPreference('petSize', size)
    bc.postMessage({ type: 'size', size: SIZE_MAP[size] })
  }
})
