import { execFile } from 'node:child_process'
import { promisify } from 'node:util'

const execFileAsync = promisify(execFile)

export interface GitBranchRow {
  name: string
  current: boolean
}

export type GitBranchesResult = {
  ok: true
  repositoryRoot: string
  currentBranch: string | null
  branches: GitBranchRow[]
  remoteUrl: string | null
} | {
  ok: false
  reason: 'no_workspace' | 'not_git_repo' | 'git_unavailable' | 'error'
  message: string
}

async function runGit(
  cwd: string,
  args: string[],
  timeout = 10_000
): Promise<{ stdout: string; stderr: string }> {
  const { stdout, stderr } = await execFileAsync('git', args, {
    cwd,
    timeout,
    maxBuffer: 1024 * 1024,
    env: { ...process.env, LC_ALL: 'C', LANG: 'C' }
  })
  return { stdout: String(stdout), stderr: String(stderr) }
}

function gitFailure(error: unknown): Extract<GitBranchesResult, { ok: false }> {
  const message = error instanceof Error ? error.message : String(error)
  if (/not a git repository/i.test(message)) {
    return { ok: false as const, reason: 'not_git_repo', message: '当前目录不是 Git 仓库。' }
  }
  if (/ENOENT/i.test(message) || /spawn git/i.test(message)) {
    return { ok: false as const, reason: 'git_unavailable', message: '未找到 Git 可执行文件。' }
  }
  return { ok: false as const, reason: 'error', message }
}

export async function getGitBranches(workspaceRoot: string): Promise<GitBranchesResult> {
  const cwd = workspaceRoot.trim()
  if (!cwd) {
    return { ok: false as const, reason: 'no_workspace', message: '未选择工作目录。' }
  }
  try {
    const repositoryRoot = (await runGit(cwd, ['rev-parse', '--show-toplevel'])).stdout.trim()
    const currentRaw = (await runGit(cwd, ['branch', '--show-current'])).stdout.trim()
    const currentBranch = currentRaw || null
    const branchLines = (await runGit(cwd, ['branch', '--format=%(refname:short)'])).stdout
      .split('\n')
      .map((line) => line.trim())
      .filter(Boolean)
    const branches: GitBranchRow[] = branchLines.map((name) => ({
      name,
      current: currentBranch === name
    }))
    // Get remote URL
    let remoteUrl: string | null = null
    try {
      remoteUrl = (await runGit(cwd, ['remote', 'get-url', 'origin'])).stdout.trim() || null
    } catch { /* no remote */ }
    return { ok: true, repositoryRoot, currentBranch, branches, remoteUrl }
  } catch (error) {
    return gitFailure(error)
  }
}

export async function switchGitBranch(
  workspaceRoot: string,
  branchName: string
): Promise<GitBranchesResult> {
  const cwd = workspaceRoot.trim()
  const branch = branchName.trim()
  if (!cwd) return { ok: false as const, reason: 'no_workspace', message: '未选择工作目录。' }
  if (!branch) return { ok: false as const, reason: 'error', message: '分支名不能为空。' }
  try {
    try {
      await runGit(cwd, ['switch', branch], 20_000)
    } catch {
      await runGit(cwd, ['checkout', branch], 20_000)
    }
    return getGitBranches(cwd)
  } catch (error) {
    return gitFailure(error)
  }
}

export async function createAndSwitchGitBranch(
  workspaceRoot: string,
  branchName: string
): Promise<GitBranchesResult> {
  const cwd = workspaceRoot.trim()
  const branch = branchName.trim()
  if (!cwd) return { ok: false as const, reason: 'no_workspace', message: '未选择工作目录。' }
  if (!branch) return { ok: false as const, reason: 'error', message: '分支名不能为空。' }
  try {
    await runGit(cwd, ['check-ref-format', '--branch', branch])
    try {
      await runGit(cwd, ['switch', '-c', branch], 20_000)
    } catch {
      await runGit(cwd, ['checkout', '-b', branch], 20_000)
    }
    return getGitBranches(cwd)
  } catch (error) {
    return gitFailure(error)
  }
}


export interface UncommittedFile {
  status: string
  file: string
  adds: number
  dels: number
}

export async function getUncommittedChanges(workspaceRoot: string): Promise<{ files: UncommittedFile[]; diff: string; repoRoot: string; error?: string; unpushedCommits: number; ahead: number; behind: number; unpushedLog: { hash: string; subject: string }[] }> {
  const cwd = workspaceRoot.trim()
  if (!cwd) return { files: [], diff: '', repoRoot: '', unpushedCommits: 0, ahead: 0, behind: 0, unpushedLog: [] }
  try {
    const repoRoot = (await runGit(cwd, ['rev-parse', '--show-toplevel'])).stdout.trim()

    const statusOut = (await runGit(cwd, ['status', '--porcelain'])).stdout
    const rawFiles: { status: string; file: string }[] = statusOut.split('\n').filter(Boolean).map(line => {
      const statusCode = line.substring(0, 2).trim()
      let filePath = line.substring(3).trim()
      if (filePath.includes(' -> ')) filePath = filePath.split(' -> ')[1]
      return { status: statusCode || 'M', file: filePath }
    })

    const numstatOut = (await runGit(cwd, ['diff', '--numstat'])).stdout
    const stagedNumstatOut = (await runGit(cwd, ['diff', '--cached', '--numstat'])).stdout
    const statMap = new Map<string, { adds: number; dels: number }>()
    for (const line of [...numstatOut.split('\n'), ...stagedNumstatOut.split('\n')].filter(Boolean)) {
      const parts = line.split('\t')
      if (parts.length < 3) continue
      const f = parts[2]
      const prev = statMap.get(f) || { adds: 0, dels: 0 }
      statMap.set(f, { adds: prev.adds + (parseInt(parts[0]) || 0), dels: prev.dels + (parseInt(parts[1]) || 0) })
    }
    const files: UncommittedFile[] = rawFiles.map(f => {
      const stats = statMap.get(f.file)
      return { ...f, adds: stats?.adds ?? 0, dels: stats?.dels ?? 0 }
    })

    const unstagedDiff = (await runGit(cwd, ['diff'])).stdout
    const stagedDiff = (await runGit(cwd, ['diff', '--cached'])).stdout
    const diff = [unstagedDiff, stagedDiff].filter(Boolean).join('\n')

    // Count ahead/behind vs upstream
    let ahead = 0
    let behind = 0
    try {
      const branch = (await runGit(cwd, ['rev-parse', '--abbrev-ref', 'HEAD'])).stdout.trim()
      ahead = parseInt((await runGit(cwd, ['rev-list', '--count', `${branch}..@{upstream}`])).stdout.trim()) || 0
    } catch { /* no upstream or no branch */ }
    try {
      const branch = (await runGit(cwd, ['rev-parse', '--abbrev-ref', 'HEAD'])).stdout.trim()
      behind = parseInt((await runGit(cwd, ['rev-list', '--count', `@{upstream}..${branch}`])).stdout.trim()) || 0
    } catch { /* no upstream or no branch */ }

    // Get unpushed commit hashes + subjects
    let unpushedCommits = 0
    let unpushedLog: { hash: string; subject: string }[] = []
    try {
      const branch = (await runGit(cwd, ['rev-parse', '--abbrev-ref', 'HEAD'])).stdout.trim()
      const logOut = (await runGit(cwd, ['log', '--oneline', `@{upstream}..${branch}`])).stdout
      unpushedLog = logOut.split('\n').filter(Boolean).map(line => {
        const space = line.indexOf(' ')
        return { hash: line.slice(0, space), subject: line.slice(space + 1) }
      })
      unpushedCommits = unpushedLog.length
    } catch { /* no upstream */ }
    // Fallback: count all commits on current branch not in any remote
    if (unpushedCommits === 0) {
      try {
        const countOut = (await runGit(cwd, ['rev-list', '--count', '--branches', '--not', '--remotes'])).stdout.trim()
        unpushedCommits = parseInt(countOut) || 0
      } catch { /* ignore */ }
    }

    return { files, diff, repoRoot, unpushedCommits, ahead, behind, unpushedLog }
  } catch (err) {
    console.error('[git:uncommitted-changes] error:', err)
    const msg = err instanceof Error ? err.message : String(err)
    return { files: [], diff: '', repoRoot: '', error: msg, unpushedCommits: 0, ahead: 0, behind: 0, unpushedLog: [] }
  }
}

export async function gitCommit(
  workspaceRoot: string,
  message: string
): Promise<{ ok: boolean; message: string }> {
  const cwd = workspaceRoot.trim()
  if (!cwd) return { ok: false, message: '未选择工作目录。' }
  if (!message.trim()) return { ok: false, message: '提交信息不能为空。' }
  try {
    // Stage all changes before commit — the desktop panel shows exactly
    // what will be committed (uncommitted changes list), so this is expected.
    await runGit(cwd, ['add', '-A'], 30_000)
    const { stdout, stderr } = await runGit(cwd, ['commit', '-m', message], 30_000)
    return { ok: true, message: stdout.trim() || stderr.trim() || '提交成功' }
  } catch (error) {
    const msg = error instanceof Error ? error.message : String(error)
    return { ok: false, message: msg }
  }
}

export async function gitPush(workspaceRoot: string): Promise<{ ok: boolean; message: string }> {
  const cwd = workspaceRoot.trim()
  if (!cwd) return { ok: false, message: '未选择工作目录。' }
  try {
    // Check if upstream is configured (rev-parse throws on no upstream)
    const branch = (await runGit(cwd, ['branch', '--show-current'])).stdout.trim()
    let upstream = ''
    try {
      upstream = (await runGit(cwd, ['rev-parse', '--abbrev-ref', '--symbolic-full-name', '@{upstream}'])).stdout.trim()
    } catch { /* no upstream */ }
    if (!upstream) {
      return { ok: false, message: `分支 '${branch}' 没有设置上游远程，请先用 git push --set-upstream 配置。` }
    }
    const { stdout, stderr } = await runGit(cwd, ['push'], 60_000)
    return { ok: true, message: stdout.trim() || stderr.trim() || '推送成功' }
  } catch (error) {
    const msg = error instanceof Error ? error.message : String(error)
    return { ok: false, message: msg }
  }
}