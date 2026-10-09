import assert from 'node:assert/strict';
import {execFileSync, spawnSync} from 'node:child_process';
import {cp, mkdir, readFile, realpath, writeFile} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';

const repository = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const stage = path.resolve(process.argv[2] || '.agents/project-app-browser/stage');
assert(stage.startsWith(`${repository}/.agents/`), 'Stage must be inside this repository’s .agents directory.');
const cli = await realpath(process.env.DEVVM_DSH_CLI || execFileSync('which', ['dsh'], {encoding:'utf8'}).trim());
const source = path.dirname(path.dirname(cli));
const manifest = JSON.parse(await readFile(path.join(source, 'package.json'), 'utf8'));
assert.equal(manifest.name, '@deepseek-ai/dsh');
assert.equal(manifest.version, '0.2.0-rc.2');
await mkdir(path.dirname(stage), {recursive:true});
await mkdir(stage);
await mkdir(path.join(repository, '.agents/tmp'), {recursive:true});
await cp(source, path.join(stage, 'cli'), {recursive:true, dereference:true});
const environment = {...process.env, TMPDIR:path.join(repository, '.agents/tmp')};
const packageRoot = path.join(stage, 'cli');
const patchFile = path.join(repository, 'patches/deepseek-harness-project-app.patch');
const patchText = await readFile(patchFile);
const patchArgs = ['--batch', '--fuzz=0', '-p1', '-d', packageRoot];
const dryRun = spawnSync('patch', [...patchArgs, '--forward', '--dry-run'], {input:patchText, env:environment});
if (dryRun.status === 0) {
  execFileSync('patch', [...patchArgs, '--forward'], {input:patchText, env:environment, stdio:['pipe','inherit','inherit']});
} else {
  execFileSync('patch', [...patchArgs, '--reverse', '--dry-run'], {input:patchText, env:environment, stdio:['pipe','inherit','inherit']});
}
const projectApp = path.join(packageRoot, 'node_modules/@devvm/dsh-project-app');
await cp(path.join(repository, 'root/.dsh/plugins/project-app'), projectApp, {recursive:true});
const registry = path.join(stage, 'registry');
await mkdir(registry);
const packed = JSON.parse(execFileSync('npm', [
  'pack', '@deepseek-ai/dsh-llm-replay@0.2.0-rc.2', '--ignore-scripts', '--json',
  '--pack-destination', registry, '--cache', path.join(stage, 'npm-cache'),
], {encoding:'utf8', env:environment}));
assert.equal(packed.length, 1);
assert.equal(packed[0].name, '@deepseek-ai/dsh-llm-replay');
assert.equal(packed[0].version, manifest.version);
await writeFile(path.join(registry, 'replay-package.json'), JSON.stringify(packed, null, 2));
const replay = path.join(packageRoot, 'node_modules/@deepseek-ai/dsh-llm-replay');
await mkdir(replay, {recursive:true});
execFileSync('tar', ['-xzf', path.join(registry, packed[0].filename), '--no-same-owner', '--strip-components=1', '-C', replay], {env:environment});
const replayEntry = path.join(replay, 'lib/index.js');
assert.equal(typeof (await import(pathToFileURL(replayEntry))).apply, 'function');
const fixture = path.join(repository, 'tests/fixtures/dsh-messages-session.v4.jsonl');
await writeFile(path.join(stage, 'browser.overlay.yml'), `- id: agent-instructions
  disabled: true
- id: session-title-llm
  disabled: true
- id: session-telemetry-otel
  disabled: true
- id: session-log-deepseek
  config:
    enabled: false
- id: ui-plugin-manager
  config:
    registryProbeEnabled: false
- id: agent-default-model
  config:
    provider: replay
    model: replay-model
- id: workspace-controller
  config:
    documentsDirectory: !!js process.cwd()
- insert:
    - id: devvm-project-app
      name: ${JSON.stringify(path.join(projectApp, 'index.mjs'))}
    - id: native-llm-replay
      name: ${JSON.stringify(replayEntry)}
      config:
        file: ${JSON.stringify(fixture)}
        paceMs: 30
        providers:
          - id: replay
            name: Replay
            models:
              - id: replay-model
                name: Replay model
                contextWindow: 128000
`);
console.log(`Prepared independent native DSH ${manifest.version} at ${stage}.`);
