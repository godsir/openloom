import { IconWinMin, IconWinMax, IconWinClose } from '../../utils/icons'

export default function WindowControls() {
  return (
    <div data-no-drag className="flex h-full">
      <button onClick={() => window.hana.windowMinimize()}
        className="w-[36px] h-full flex items-center justify-center text-[rgba(255,255,255,0.35)] hover:text-[rgba(255,255,255,0.8)] hover:bg-[rgba(255,255,255,0.06)] transition-colors"
        aria-label="最小化"><IconWinMin size={10} /></button>
      <button onClick={() => window.hana.windowMaximize()}
        className="w-[36px] h-full flex items-center justify-center text-[rgba(255,255,255,0.35)] hover:text-[rgba(255,255,255,0.8)] hover:bg-[rgba(255,255,255,0.06)] transition-colors"
        aria-label="最大化"><IconWinMax size={10} /></button>
      <button onClick={() => window.hana.windowClose()}
        className="w-[36px] h-full flex items-center justify-center text-[rgba(255,255,255,0.35)] hover:text-white hover:bg-[var(--red)] transition-colors"
        aria-label="关闭"><IconWinClose size={10} /></button>
    </div>
  )
}
