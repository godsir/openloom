const fs = require('fs')
const path = require('path')

// electron-builder writes prerelease metadata using its default `latest`
// filenames. The dedicated beta update feed must instead expose the channel
// names electron-updater requests from the stable `beta` release.
const dist = path.join(__dirname, '..', 'dist')
const renames = [
  ['latest.yml', 'beta.yml'],
  ['latest-mac.yml', 'beta-mac.yml'],
  ['latest-linux.yml', 'beta-linux.yml'],
]

for (const [source, destination] of renames) {
  const sourcePath = path.join(dist, source)
  if (fs.existsSync(sourcePath)) {
    fs.renameSync(sourcePath, path.join(dist, destination))
    console.log(`Prepared beta update metadata: ${destination}`)
  }
}
