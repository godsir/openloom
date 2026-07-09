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
