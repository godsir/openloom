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
    // 同步缩放透明窗口本身（此前只缩放 canvas，窗口仍是原尺寸，
    // 宠物缩在大窗口一角，占位与可见精灵不符）。与 PetTab.changeSize 对齐。
    window.loom.resizePet(SIZE_MAP[size])
    bc.postMessage({ type: 'size', size: SIZE_MAP[size] })
  }
})
