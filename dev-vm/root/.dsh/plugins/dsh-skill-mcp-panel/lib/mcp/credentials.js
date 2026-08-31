/**
 * dsh-skill-mcp-panel —— MCP 凭据存储管理器。
 *
 * 将 Bearer Token 存入 ~/.dsh/.credentials.yaml（与 DSH 模型凭据文件相同），
 * 避免在 cordis.patch.yml 中明文存储敏感令牌。
 */
import { readFile, writeFile, chmod, mkdir } from "node:fs/promises";
import { dirname, join } from "node:path";
import { parseDocument } from "yaml";
import { resolveDshHome } from "@deepseek-ai/dsh-home-paths";

export function getCredentialsPath() {
    const home = resolveDshHome();
    return join(home, ".credentials.yaml");
}

export function credentialRefForServer(serverName) {
    // 十六进制保留 serverName 的大小写与每个字符，同时满足凭据 ref 格式。
    const encodedName = Buffer.from(String(serverName ?? ""), "utf8").toString("hex").toUpperCase();
    return `MCP_${encodedName}_TOKEN`;
}

function parseCredentials(content, path) {
    const doc = parseDocument(content);
    if (doc.errors.length > 0) {
        throw new Error("无法解析凭据文件（" + path + "）：" + String(doc.errors[0]?.message ?? doc.errors[0]));
    }
    return doc;
}

function isMissingFile(error) {
    return error !== null && typeof error === "object" && error.code === "ENOENT";
}

export async function readCredential(ref) {
    const path = getCredentialsPath();
    let content;
    try {
        content = await readFile(path, "utf8");
    } catch (error) {
        if (isMissingFile(error)) return undefined;
        throw error;
    }
    const data = parseCredentials(content, path).toJS();
    return data?.refs?.[ref] ?? undefined;
}

export async function setCredential(ref, token) {
    if (!token || typeof token !== "string" || !token.trim()) return;
    const path = getCredentialsPath();
    await mkdir(dirname(path), { recursive: true });
    let content = "";
    try {
        content = await readFile(path, "utf8");
    } catch (e) {
        if (e && e.code !== "ENOENT") throw e;
    }

    let doc;
    if (content.trim()) {
        doc = parseCredentials(content, path);
    } else {
        doc = parseDocument("version: 1\nrefs:\n");
    }

    if (!doc.has("version")) doc.set("version", 1);
    if (!doc.has("refs")) doc.set("refs", doc.createNode({}));

    doc.setIn(["refs", ref], token.trim());
    await writeFile(path, doc.toString(), "utf8");
    if (process.platform !== "win32") {
        await chmod(path, 0o600);
    }
}

export async function unsetCredential(ref) {
    const path = getCredentialsPath();
    let content;
    try {
        content = await readFile(path, "utf8");
    } catch (error) {
        if (isMissingFile(error)) return;
        throw error;
    }
    const doc = parseCredentials(content, path);
    if (doc.hasIn(["refs", ref])) {
        doc.deleteIn(["refs", ref]);
        await writeFile(path, doc.toString(), "utf8");
        if (process.platform !== "win32") {
            await chmod(path, 0o600);
        }
    }
}
