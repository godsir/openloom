// 生成 electron-updater 兼容的 latest.yml
const fs = require('fs');
const crypto = require('crypto');
const path = require('path');
const pkg = require('../package.json');

const exeName = `openLoom.Setup.${pkg.version}.exe`;
const exe = path.join(__dirname, '..', 'dist', exeName);

if (!fs.existsSync(exe)) {
  console.error('Installer not found:', exe);
  process.exit(1);
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

fs.writeFileSync(path.join(__dirname, '..', 'dist', 'latest.yml'), yml);
console.log('latest.yml generated for', exeName);
