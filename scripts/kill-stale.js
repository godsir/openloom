// Kill stale loom-server processes before dev start
const { execSync } = require('child_process');

try {
  if (process.platform === 'win32') {
    execSync('taskkill /f /im loom-server.exe 2>nul', { stdio: 'ignore' });
  } else {
    execSync('pkill -f loom-server 2>/dev/null || true', { stdio: 'ignore' });
  }
  console.log('[predev] stale loom-server cleaned');
} catch {
  // No stale process found — ok
}
