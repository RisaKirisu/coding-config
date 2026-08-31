import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";

const home = await mkdtemp(join(tmpdir(), "dsh-panel-credentials-"));
process.env.DSH_HOME = home;

const {
  credentialRefForServer,
  readCredential,
  unsetCredential
} = await import("./lib/mcp/credentials.js");

try {
  const refs = ["foo", "FOO", "foo-bar", "foo_bar"].map(credentialRefForServer);
  assert.equal(new Set(refs).size, refs.length);
  assert.ok(refs.every((ref) => /^[A-Za-z_][A-Za-z0-9_]*$/.test(ref)));

  assert.equal(await readCredential(refs[0]), undefined);
  await unsetCredential(refs[0]);

  await writeFile(join(home, ".credentials.yaml"), "version: 1\nrefs: [\n", "utf8");
  await assert.rejects(() => readCredential(refs[0]), /无法解析凭据文件/);
  await assert.rejects(() => unsetCredential(refs[0]), /无法解析凭据文件/);

  console.log("credential reference and error propagation tests passed");
} finally {
  await rm(home, { recursive: true, force: true });
}
