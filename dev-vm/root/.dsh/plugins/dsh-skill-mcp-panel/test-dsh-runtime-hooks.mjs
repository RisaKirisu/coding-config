// Route bare @deepseek-ai/* imports to the installed dsh runtime, as dsh's
// profile resolver does when the plugin runs inside a profile.
import { registerHooks } from 'node:module'
import { pathToFileURL } from 'node:url'
import { join } from 'node:path'

const runtimeModules = join(process.env.DSH_RUNTIME_DIR ?? '/usr/local/lib/node_modules/@deepseek-ai/dsh', 'node_modules')
const runtimeParent = pathToFileURL(join(runtimeModules, '@deepseek-ai', 'dsh-app-boot', 'package.json')).href

registerHooks({
  resolve(specifier, context, nextResolve) {
    if (specifier.startsWith('@deepseek-ai/'))
      return nextResolve(specifier, { ...context, parentURL: runtimeParent })
    return nextResolve(specifier, context)
  },
})
