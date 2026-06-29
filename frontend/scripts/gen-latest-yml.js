// 生成 electron-updater 兼容的 latest.yml
const fs = require('fs');
const crypto = require('crypto');
const path = require('path');
const pkg = require('../package.json');
const distDir = path.join(__dirname, '..', 'dist');

// info.nsi 的 INSTALL_OUTPUT_NAME 可能与 package.json version 不完全一致
// （beta version 0.4.1-beta.23 vs info.nsi 硬编码 0.4.1），glob 找实际 exe
let exeName = `openLoom.Setup.${pkg.version}.exe`;
let exe = path.join(distDir, exeName);
if (!fs.existsSync(exe)) {
  const exes = fs.readdirSync(distDir).filter(f => /^openLoom\.Setup\..+\.exe$/.test(f));
  if (exes.length === 0) {
    console.error('No installer found in dist/');
    process.exit(1);
  }
  exeName = exes[0];
  exe = path.join(distDir, exeName);
}

const buf = fs.readFileSync(exe);
const sha512 = crypto.createHash('sha512').update(buf).digest('base64');
const yml = `version: ${pkg.version}
files:
  - url: ${exeName}
    sha512: ${sha512}
    size: ${buf.length}
path: ${exeName}
sha512: ${sha512}
releaseDate: '${new Date().toISOString()}'
`;
fs.writeFileSync(path.join(distDir, 'latest.yml'), yml);
console.log('latest.yml generated for', exeName, 'version', pkg.version);
