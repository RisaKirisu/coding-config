/**
 * dsh-skill-mcp-panel —— MCP 宿主服务（mcpManager）。
 */
import { TypertRemoteService } from "@deepseek-ai/dsh-typert-protocol";
import { credentialRef } from "@deepseek-ai/dsh-credentials";
import { fileURLToPath } from "node:url";
import { join, resolve } from "node:path";
import { MCP_PLUGIN_NAME, extractManagedRows, listMcpPatchRows, readPatchFile, writeManagedRows } from "../patch-editor.js";
import { credentialRefForSecret, describeSchemaError, inputFromPatchRow, keyValueRowsOf, patchRowToView, secretRefsFromPatchRow, serverNameFromRowId, toPatchRow } from "./model.js";
import { mcpRemovePayloadSchema, mcpSavePayloadSchema, mcpSetEnabledPayloadSchema, mcpTestPayloadSchema } from "./wire.js";
import { fiberPhaseOf, getLoaderEntry, mcpToolCount, waitForLoaderState } from "./status.js";
import { probeMcpServer } from "./probe.js";
function stripUndefined(value) {
    if (Array.isArray(value))
        return value.map((item) => stripUndefined(item));
    if (value !== null && typeof value === "object") {
        const out = {};
        for (const [key, item] of Object.entries(value)) {
            if (item === undefined)
                continue;
            out[key] = stripUndefined(item);
        }
        return out;
    }
    return value;
}
const MANAGED_ROW_IDS = new Set();
function isManagedRow(row) {
    return typeof row.id === "string" && row.id.startsWith("panel-mcp-");
}
/**
 * handler 内校验 payload：失败抛「字段：原因」的中文错误。
 * 网关边界的 codec 是宽松 schema（见 wire.ts），校验必须在这里做，
 * 否则失败会退化成宿主那句泛化的 `failed boundary validation`。
 */
function parseWirePayload(schema, raw, label) {
    const result = schema.safeParse(raw);
    if (result.success)
        return result.data;
    throw new Error(label + "：" + describeSchemaError(result.error));
}
export class McpManagerGateway extends TypertRemoteService {
    constructor(ctx) {
        super(ctx, "mcpManager");
    }
    get C() {
        return this.ctx;
    }
    patchPath() {
        const base = this.C.baseUrl;
        if (typeof base === "string" && base.length > 0) {
            try {
                const url = new URL(base);
                if (url.protocol === "file:")
                    return join(fileURLToPath(url), "cordis.patch.yml");
            }
            catch {
                // fall through to package-location fallback
            }
        }
        const packageDir = fileURLToPath(new URL("../../", import.meta.url));
        return join(resolve(packageDir, "../.."), "cordis.patch.yml");
    }
    async readRows() {
        const path = this.patchPath();
        const raw = await readPatchFile(path);
        const managed = extractManagedRows(raw);
        const allMcp = listMcpPatchRows(raw);
        const managedIds = new Set(managed.map((row) => row.id).filter((id) => typeof id === "string"));
        const external = allMcp.filter((row) => typeof row.id === "string" && !managedIds.has(row.id));
        return { path, raw, managed, external };
    }
    decorate(row, managed, entry, enabled) {
        const view = patchRowToView(row);
        if (view === undefined)
            return undefined;
        const fiberPhase = fiberPhaseOf(entry?.fiber?.state);
        return stripUndefined({
            ...view,
            enabled,
            managed,
            fiberPhase,
            toolCount: enabled ? mcpToolCount(this.C, view.serverName) : 0
        });
    }
    async list() {
        let patch = { path: this.patchPath(), ok: false, error: null };
        try {
            const { path, managed, external } = await this.readRows();
            patch = { path, ok: true, error: null };
            const servers = [];
            for (const row of managed) {
                const entry = typeof row.id === "string" ? getLoaderEntry(this.C, row.id) : undefined;
                const view = this.decorate(row, true, entry, row.disabled !== true);
                if (view !== undefined)
                    servers.push(view);
            }
            const externalServers = [];
            for (const row of external) {
                const entry = typeof row.id === "string" ? getLoaderEntry(this.C, row.id) : undefined;
                const view = this.decorate(row, false, entry, row.disabled !== true);
                if (view !== undefined)
                    externalServers.push(view);
            }
            return { servers, externalServers, patch };
        }
        catch (error) {
            return { servers: [], externalServers: [], patch: { ...patch, error: error instanceof Error ? error.message : String(error) } };
        }
    }
    findRowByServerName(rows, serverName) {
        return rows.find((row) => serverNameFromRowId(row.id) === serverName || (row.config?.serverName === serverName && isManagedRow(row)));
    }
    /** patch 行 → 完整输入，secret 行的值从凭据存储解析。 */
    async configInputFromRow(row) {
        const input = inputFromPatchRow(row);
        const secretRefs = secretRefsFromPatchRow(row);
        for (const item of keyValueRowsOf(input)) {
            if (!item.secret)
                continue;
            const stored = await this.C.credentials.resolve(credentialRef(secretRefs[item.key]));
            item.value = stored?.value ?? "";
        }
        return input;
    }
    /**
     * 把输入中的 secret 值写入凭据存储。value 为空的 secret 行沿用旧行已存储的值；
     * 改名时把旧引用的值复制到新引用。
     */
    async storeSecrets(input, previous) {
        const previousRefs = previous === undefined ? {} : secretRefsFromPatchRow(previous);
        for (const item of keyValueRowsOf(input)) {
            if (!item.secret)
                continue;
            const ref = credentialRef(credentialRefForSecret(input.serverName, item.key));
            if (item.value !== "") {
                await this.C.credentials.set(ref, item.value);
                continue;
            }
            const previousRef = previousRefs[item.key];
            if (previousRef === undefined)
                throw new Error('secret "' + item.key + '" 需要填写值');
            if (previousRef === ref)
                continue;
            const stored = await this.C.credentials.resolve(credentialRef(previousRef));
            if (stored === undefined)
                throw new Error('secret "' + item.key + '" 的已存储值不存在，请重新填写');
            await this.C.credentials.set(ref, stored.value);
        }
    }
    /** 删除旧行引用、但新行不再引用的凭据。 */
    async removeUnusedSecrets(previous, next) {
        if (previous === undefined)
            return;
        const nextRefs = new Set(next === undefined ? [] : Object.values(secretRefsFromPatchRow(next)));
        for (const ref of Object.values(secretRefsFromPatchRow(previous))) {
            if (!nextRefs.has(ref))
                await this.C.credentials.unset(credentialRef(ref));
        }
    }
    async save(rawPayload) {
        const payload = parseWirePayload(mcpSavePayloadSchema, rawPayload, "MCP 配置无效");
        const input = payload.input;
        const previousName = payload.previousServerName ?? input.serverName;
        const { managed, external } = await this.readRows();
        for (const row of external) {
            const name = row.config?.serverName;
            if (name === input.serverName)
                throw new Error('serverName "' + input.serverName + '" 已被 cordis.patch.yml 中的外部 MCP 行占用，请在文件中手动处理');
        }
        for (const row of managed) {
            const name = row.config?.serverName;
            if (name === input.serverName && serverNameFromRowId(row.id) !== previousName) {
                throw new Error('serverName "' + input.serverName + '" 已存在（受管行 ' + String(row.id) + "）");
            }
        }
        const previous = managed.find((row) => serverNameFromRowId(row.id) === previousName || row.config?.serverName === previousName);
        if (payload.previousServerName !== undefined && previous === undefined) {
            throw new Error('要编辑的 MCP 行不存在："' + previousName + '"');
        }
        const enabled = previous !== undefined ? previous.disabled !== true : payload.enabled;
        await this.storeSecrets(input, previous);
        const nextRow = toPatchRow(input, enabled);
        const nextRows = managed.filter((row) => serverNameFromRowId(row.id) !== previousName && row.config?.serverName !== previousName);
        nextRows.push(nextRow);
        nextRows.sort((a, b) => String(a.config?.serverName ?? "").localeCompare(String(b.config?.serverName ?? "")));
        await writeManagedRows(this.patchPath(), nextRows);
        const reconciled = enabled
            ? await waitForLoaderState(this.C, nextRow.id, (entry) => entry !== undefined && entry.disabled !== true)
            : await waitForLoaderState(this.C, nextRow.id, (entry) => entry !== undefined && entry.disabled === true);
        await this.removeUnusedSecrets(previous, nextRow);
        const entry = getLoaderEntry(this.C, nextRow.id);
        const server = this.decorate(nextRow, true, entry, enabled);
        if (server === undefined)
            throw new Error("写入成功但生成的 MCP 行无效");
        return { server, reconciled };
    }
    async removeServer(rawPayload) {
        const payload = parseWirePayload(mcpRemovePayloadSchema, rawPayload, "删除参数无效");
        const { managed } = await this.readRows();
        const row = managed.find((candidate) => serverNameFromRowId(candidate.id) === payload.serverName || candidate.config?.serverName === payload.serverName);
        if (row === undefined) {
            throw new Error('MCP 行 "' + payload.serverName + '" 不存在或不是面板受管行（外部行请在 cordis.patch.yml 中手动删除）');
        }
        const nextRows = managed.filter((candidate) => candidate !== row);
        await writeManagedRows(this.patchPath(), nextRows);
        const reconciled = await waitForLoaderState(this.C, row.id, (entry) => entry === undefined);
        await this.removeUnusedSecrets(row, undefined);
        return { ok: true, reconciled };
    }
    async setEnabled(rawPayload) {
        const payload = parseWirePayload(mcpSetEnabledPayloadSchema, rawPayload, "启用参数无效");
        const { managed } = await this.readRows();
        const row = managed.find((candidate) => serverNameFromRowId(candidate.id) === payload.serverName || candidate.config?.serverName === payload.serverName);
        if (row === undefined)
            throw new Error('MCP 行 "' + payload.serverName + '" 不存在或不是面板受管行');
        row.disabled = !payload.enabled;
        await writeManagedRows(this.patchPath(), managed);
        const reconciled = payload.enabled
            ? await waitForLoaderState(this.C, row.id, (entry) => entry !== undefined && entry.disabled !== true)
            : await waitForLoaderState(this.C, row.id, (entry) => entry !== undefined && entry.disabled === true);
        const entry = getLoaderEntry(this.C, row.id);
        const server = this.decorate(row, true, entry, payload.enabled);
        if (server === undefined)
            throw new Error("写入成功但生成的 MCP 行无效");
        return { server, reconciled };
    }
    async test(rawPayload) {
        const payload = parseWirePayload(mcpTestPayloadSchema, rawPayload, "测试参数无效");
        if (payload !== null && typeof payload === "object" && !("transport" in payload) && "serverName" in payload) {
            const { managed, external } = await this.readRows();
            const row = [...managed, ...external].find((candidate) => candidate.config?.serverName === payload.serverName || serverNameFromRowId(candidate.id) === payload.serverName);
            if (row === undefined)
                throw new Error('MCP 行 "' + String(payload.serverName) + '" 不存在');
            return probeMcpServer(await this.configInputFromRow(row));
        }
        return probeMcpServer(payload);
    }
    reload() {
        return this.list();
    }
}
// 供 CLI 复用：判断一个 patch 行是否由面板管理。
export { MANAGED_ROW_IDS, isManagedRow, MCP_PLUGIN_NAME };
