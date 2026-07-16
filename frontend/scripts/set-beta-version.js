// Give every CI build a monotonic prerelease version before electron-builder
// generates channel metadata (beta.yml). Kept as a script so all runners use
// identical JSON formatting and version semantics.
const fs = require('fs')
const path = require('path')

const runNumber = process.env.GITHUB_RUN_NUMBER
if (!runNumber) throw new Error('GITHUB_RUN_NUMBER is required for beta builds')

const packagePath = path.join(__dirname, '..', 'package.json')
const pkg = JSON.parse(fs.readFileSync(packagePath, 'utf8'))
pkg.version = `${pkg.version.replace(/-beta\.\d+$/, '')}-beta.${runNumber}`
fs.writeFileSync(packagePath, `${JSON.stringify(pkg, null, 2)}\n`)
console.log(`Beta version: ${pkg.version}`)
