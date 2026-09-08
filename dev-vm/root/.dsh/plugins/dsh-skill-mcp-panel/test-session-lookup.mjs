import assert from 'node:assert/strict'
import test from 'node:test'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'

const __dirname = dirname(fileURLToPath(import.meta.url))
const clientPath = join(__dirname, 'lib', 'client.js')

test('browser current-session lookup works without removed currentProvideInfo across DSH 0.1.2 and legacy stores', async () => {
  let registeredBundle
  globalThis.window = {
    __ModuleLoader__: {
      load: (def) => {
        registeredBundle = def
      },
    },
  }

  await import(`${clientPath}?v=${Date.now()}`)
  assert.ok(registeredBundle, 'client.js must register with window.__ModuleLoader__')

  const fakeRequire = () => ({ jsx: () => ({}) })
  const clientModule = registeredBundle.factory(fakeRequire)
  assert.ok(typeof clientModule.apply === 'function', 'clientModule must export apply')
  assert.ok(
    typeof clientModule.resolveCurrentSessionId === 'function',
    'clientModule must export resolveCurrentSessionId helper',
  )

  const { resolveCurrentSessionId } = clientModule

  // 1. DSH 0.1.2: list.current is the active session id (authoritative)
  assert.equal(
    resolveCurrentSessionId({
      selection: { getSnapshot: () => ({ sessionId: 'sess-sel-1' }) },
      list: { getSnapshot: () => ({ current: 'sess-list-1' }) },
    }),
    'sess-list-1',
  )

  // 2. DSH 0.1.2: persisted selection restores when list has no current
  assert.equal(
    resolveCurrentSessionId({
      selection: { getSnapshot: () => ({ sessionId: 'sess-sel-2' }) },
      list: { getSnapshot: () => ({}) },
    }),
    'sess-sel-2',
  )

  // 3. DSH 0.1.2 list store with currentAddress
  assert.equal(
    resolveCurrentSessionId({
      selection: { getSnapshot: () => ({}) },
      list: { getSnapshot: () => ({ currentAddress: { sessionId: 'sess-addr-3' } }) },
    }),
    'sess-addr-3',
  )

  // 4. DSH 0.1.2: list.current without selection store
  assert.equal(
    resolveCurrentSessionId({
      list: { getSnapshot: () => ({ current: 'sess-list-4' }) },
    }),
    'sess-list-4',
  )

  // 4a. list snapshot sessionId fallback when current/currentAddress absent
  assert.equal(
    resolveCurrentSessionId({
      list: { getSnapshot: () => ({ sessionId: 'sess-list-snap-4a' }) },
    }),
    'sess-list-snap-4a',
  )

  // 4b. generic sessions store snapshot with current
  assert.equal(
    resolveCurrentSessionId({
      getSnapshot: () => ({ current: 'sess-generic-4b' }),
    }),
    'sess-generic-4b',
  )

  // 4c. generic sessions store snapshot with sessionId
  assert.equal(
    resolveCurrentSessionId({
      getSnapshot: () => ({ sessionId: 'sess-generic-4c' }),
    }),
    'sess-generic-4c',
  )

  // 4d. direct current string property
  assert.equal(resolveCurrentSessionId({ current: 'sess-direct-4d' }), 'sess-direct-4d')

  // 4e. direct sessionId string property
  assert.equal(resolveCurrentSessionId({ sessionId: 'sess-direct-4e' }), 'sess-direct-4e')

  // 5. Legacy DSH 0.1.1 currentProvideInfo
  assert.equal(
    resolveCurrentSessionId({
      currentProvideInfo: { getSnapshot: () => ({ sessionId: 'sess-legacy-5' }) },
    }),
    'sess-legacy-5',
  )

  // 6. Empty / missing session returns undefined without throwing
  assert.equal(resolveCurrentSessionId(undefined), undefined)
  assert.equal(resolveCurrentSessionId(null), undefined)
  assert.equal(
    resolveCurrentSessionId({
      selection: { getSnapshot: () => ({}) },
      list: { getSnapshot: () => ({}) },
    }),
    undefined,
  )
  // A store object present without a usable getSnapshot (the removed-API half
  // shape: currentProvideInfo existed on 0.1.1, and its getSnapshot access is
  // what threw on 0.1.2) must be skipped, never dereferenced.
  assert.equal(
    resolveCurrentSessionId({
      currentProvideInfo: {},
      selection: { getSnapshot: () => ({}) },
      list: { getSnapshot: () => ({}) },
    }),
    undefined,
  )
  // A non-callable getSnapshot must also be skipped: only a function may be
  // invoked, or the probe itself throws.
  assert.equal(
    resolveCurrentSessionId({
      currentProvideInfo: { getSnapshot: 'not-a-function' },
      selection: { getSnapshot: () => ({}) },
      list: { getSnapshot: () => ({}) },
    }),
    undefined,
  )

  // 7. Test section face wiring inside apply(ctx)
  let sectionFaceFn
  const ctx = {
    get: (name) => {
      if (name === 'sessions') {
        return {
          selection: { getSnapshot: () => ({ sessionId: 'sess-applied-5' }) },
          list: { getSnapshot: () => ({ current: 'sess-applied-5' }) },
        }
      }
      return undefined
    },
    effect: () => {},
    locale: { register: () => {}, bind: () => () => '' },
    remote: { $mount: () => Promise.resolve() },
    slots: {
      inject: (_name, cb) => cb(),
      register: (opts) => {
        if (opts.id === 'skills') sectionFaceFn = opts.inject
      },
    },
  }

  clientModule.apply(ctx)
  assert.ok(typeof sectionFaceFn === 'function', 'skills settings section must register inject function')

  const face = sectionFaceFn()
  assert.equal(
    face.currentSessionId(),
    'sess-applied-5',
    'face.currentSessionId() must return active session ID in DSH 0.1.2',
  )
})
