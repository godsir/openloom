/**
 * 代码围栏检测工具（纯函数，无依赖，便于单测）。
 *
 * 供流式 markdown 渲染判断"当前是否正在输出代码块内部"，从而把新行渲染进
 * 代码块、而非拼在已自动闭合的 </code> 之后（避免代码行先闪在块外再吸进去）。
 */

/**
 * 返回 text 中"最后一个未闭合代码围栏块"的起始字符下标；若无未闭合围栏返回 -1。
 *
 * 支持 ``` 与 ~~~ 围栏（允许行首缩进与开栏信息串）。闭栏须与开栏同字符且
 * 长度不小于开栏（CommonMark 近似）。
 */
export function unclosedFenceStart(text: string): number {
  let inside = false
  let fenceChar = ''
  let fenceLen = 0
  let blockStart = -1
  let offset = 0
  for (const line of text.split('\n')) {
    const m = /^[ \t]*(```+|~~~+)/.exec(line)
    if (m) {
      const marker = m[1]
      if (!inside) {
        inside = true
        fenceChar = marker[0]
        fenceLen = marker.length
        blockStart = offset
      } else if (marker[0] === fenceChar && marker.length >= fenceLen) {
        inside = false
      }
    }
    offset += line.length + 1
  }
  return inside ? blockStart : -1
}
