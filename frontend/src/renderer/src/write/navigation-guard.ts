import { loomRpc } from '../services/jsonrpc'
import { useStore } from '../stores'
import { useWriteStore } from '../stores/write'

export async function guardWriteNavigation(): Promise<boolean> {
  const state = useWriteStore.getState()
  if (state.saveStatus === 'saved' || !state.workspaceRoot || !state.activeFilePath) return true

  const snapshot = {
    workspaceRoot: state.workspaceRoot,
    filePath: state.activeFilePath,
    content: state.fileContent,
  }

  try {
    useWriteStore.getState().setSaveStatus('saving')
    await loomRpc('vfs.write_file', {
      workspace_root: snapshot.workspaceRoot,
      path: snapshot.filePath,
      content: snapshot.content,
    })
    const current = useWriteStore.getState()
    if (
      current.workspaceRoot === snapshot.workspaceRoot &&
      current.activeFilePath === snapshot.filePath &&
      current.fileContent === snapshot.content
    ) {
      current.setSaveStatus('saved')
      return true
    }
    current.setSaveStatus('dirty')
    return false
  } catch {
    useWriteStore.getState().setSaveStatus('error')
    return useStore.getState().showConfirm(
      '未保存的修改',
      '当前文档保存失败。是否放弃未保存的修改并继续？',
      true,
    )
  }
}
