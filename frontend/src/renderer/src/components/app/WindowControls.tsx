export default function WindowControls() {
  return (
    <div
      className="flex items-center gap-1.5 px-3 py-1"
      style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}
    >
      <button
        onClick={() => window.hana.windowMinimize()}
        className="w-3 h-3 rounded-full bg-yellow-500 hover:bg-yellow-400 transition-colors"
        aria-label="最小化"
      />
      <button
        onClick={() => window.hana.windowMaximize()}
        className="w-3 h-3 rounded-full bg-green-500 hover:bg-green-400 transition-colors"
        aria-label="最大化"
      />
      <button
        onClick={() => window.hana.windowClose()}
        className="w-3 h-3 rounded-full bg-red-500 hover:bg-red-400 transition-colors"
        aria-label="关闭"
      />
    </div>
  )
}
