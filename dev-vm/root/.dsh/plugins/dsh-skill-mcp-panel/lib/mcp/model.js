/**
 * dsh-skill-mcp-panel —— MCP 服务器配置模型。
 *
 * v1 仅全局生效：模型不包含 scope。
 *
 * env/headers 在面板输入中是行列表 `{ key, value, secret }`，列表即完整集合。
 * 非 secret 行的值以明文写入 patch；secret 行只在 patch 中写凭据引用
 * （`secretEnv` / `secretHeaders`：key → ref），值存放在 DSH 凭据存储中，
 * 由 `dsh-skill-mcp-panel/mcp-client` 入口在启动时解析。
 * secret 行的 value 为空表示沿用已存储的值。
 */
import { z } from "zod";
import { MANAGED_ROW_ID_PREFIX, MCP_ROW_NAMES, PANEL_MCP_CLIENT_NAME } from "../patch-editor.js";
export const SERVER_NAME_RE = /^[A-Za-z0-9_-]{1,32}$/;
export const DEFAULT_TOOL_CALL_TIMEOUT_MS = 60000;
export const DEFAULT_RECONNECT = {
    enabled: true,
    initialDelayMs: 500,
    maxDelayMs: 30000,
    maxAttempts: 10
};
const serverNameSchema = z.string().regex(SERVER_NAME_RE, "serverName 只能包含 1-32 位字母、数字、下划线或连字符");
const keyValueRowSchema = z.object({
    key: z.string().trim().min(1, "键不能为空"),
    value: z.string().default(""),
    secret: z.boolean().default(false)
});
const keyValueRowsSchema = z.array(keyValueRowSchema).default([]).superRefine((rows, context) => {
    const seen = new Set();
    for (const row of rows) {
        if (seen.has(row.key))
            context.addIssue({ code: "custom", message: '键 "' + row.key + '" 重复' });
        seen.add(row.key);
    }
});
const reconnectSchema = z.object({
    enabled: z.boolean().default(DEFAULT_RECONNECT.enabled),
    initialDelayMs: z.number().int().min(1).default(DEFAULT_RECONNECT.initialDelayMs),
    maxDelayMs: z.number().int().min(1).default(DEFAULT_RECONNECT.maxDelayMs),
    maxAttempts: z.number().int().min(1).default(DEFAULT_RECONNECT.maxAttempts)
}).default({ ...DEFAULT_RECONNECT });
export const stdioServerSchema = z.object({
    serverName: serverNameSchema,
    transport: z.literal("stdio"),
    command: z.string().min(1, "命令不能为空"),
    args: z.array(z.string()).default([]),
    env: keyValueRowsSchema,
    cwd: z.string().default(""),
    toolCallTimeoutMs: z.number().int().min(1).default(DEFAULT_TOOL_CALL_TIMEOUT_MS),
    failOnStartupError: z.boolean().default(false),
    reconnect: reconnectSchema
});
export const httpServerSchema = z.object({
    serverName: serverNameSchema,
    transport: z.literal("streamable-http"),
    url: z.string().url("服务器地址必须是合法 URL"),
    headers: keyValueRowsSchema,
    toolCallTimeoutMs: z.number().int().min(1).default(DEFAULT_TOOL_CALL_TIMEOUT_MS),
    failOnStartupError: z.boolean().default(false),
    reconnect: reconnectSchema
});
export const mcpServerInputSchema = z.discriminatedUnion("transport", [stdioServerSchema, httpServerSchema]);
/**
 * 把 zod 的校验失败压成一行「字段：原因」。
 *
 * 网关边界（`codec.create().parse`）失败时宿主只回一句泛化的
 * `wire field "payload" failed boundary validation`，zod 的字段说明到不了前端，
 * 用户看到的就是天书（典型：名称里带空格）。所以字段校验放在 handler 里做，
 * 用这个函数生成可读错误，前端原样展示。
 */
export function describeSchemaError(error) {
    const issues = error?.issues;
    if (!Array.isArray(issues) || issues.length === 0)
        return String(error?.message ?? error);
    return issues
        .map((issue) => {
        const path = Array.isArray(issue?.path) && issue.path.length > 0 ? issue.path.join(".") : "payload";
        return path + "：" + String(issue?.message ?? "无效");
    })
        .join("；");
}
/** 面板行 id ↔ serverName。 */
export function rowIdForServerName(serverName) {
    return MANAGED_ROW_ID_PREFIX + serverName;
}
export function serverNameFromRowId(id) {
    if (typeof id !== "string" || !id.startsWith(MANAGED_ROW_ID_PREFIX))
        return undefined;
    const name = id.slice(MANAGED_ROW_ID_PREFIX.length);
    return SERVER_NAME_RE.test(name) ? name : undefined;
}
/**
 * secret 值在凭据存储中的引用名。凭据引用必须是 shell 标识符，
 * 十六进制编码保留 serverName 与 key 的大小写和每个字符，避免不同 key 撞名。
 */
export function credentialRefForSecret(serverName, key) {
    const hex = (value) => Buffer.from(value, "utf8").toString("hex").toUpperCase();
    return "MCP_" + hex(serverName) + "_" + hex(key);
}
/** transport 对应的明文字段名与 secret 引用字段名。 */
export function keyValueFields(transport) {
    return transport === "stdio" ? { plain: "env", secret: "secretEnv" } : { plain: "headers", secret: "secretHeaders" };
}
function normalizeReconnect(input) {
    return {
        enabled: input.reconnect.enabled,
        initialDelayMs: input.reconnect.initialDelayMs,
        maxDelayMs: input.reconnect.maxDelayMs,
        maxAttempts: input.reconnect.maxAttempts
    };
}
/** 行列表 → 明文 map 与 secret 引用 map。 */
function splitKeyValueRows(serverName, rows) {
    const plain = {};
    const secretRefs = {};
    for (const row of rows) {
        if (row.secret)
            secretRefs[row.key] = credentialRefForSecret(serverName, row.key);
        else
            plain[row.key] = row.value;
    }
    return { plain, secretRefs };
}
/** 面板输入 → 面板 MCP 入口配置（secret 只写引用）。 */
export function toPanelClientConfig(input) {
    const common = {
        serverName: input.serverName,
        toolCallTimeoutMs: input.toolCallTimeoutMs,
        failOnStartupError: input.failOnStartupError,
        reconnect: normalizeReconnect(input)
    };
    if (input.transport === "stdio") {
        const { plain, secretRefs } = splitKeyValueRows(input.serverName, input.env);
        return {
            ...common,
            transport: "stdio",
            command: input.command,
            args: input.args,
            env: plain,
            secretEnv: secretRefs,
            cwd: input.cwd
        };
    }
    const { plain, secretRefs } = splitKeyValueRows(input.serverName, input.headers);
    return {
        ...common,
        transport: "streamable-http",
        url: input.url,
        headers: plain,
        secretHeaders: secretRefs
    };
}
/** 面板输入 → cordis.patch.yml 行。 */
export function toPatchRow(input, enabled = true) {
    return {
        id: rowIdForServerName(input.serverName),
        name: PANEL_MCP_CLIENT_NAME,
        ...(enabled ? {} : { disabled: true }),
        config: toPanelClientConfig(input)
    };
}
/** 读取 patch 行中的 config（宽松，坏行返回 undefined）。 */
export function configFromPatchRow(row) {
    if (row === undefined || !MCP_ROW_NAMES.has(row.name))
        return undefined;
    if (row.config === null || typeof row.config !== "object" || Array.isArray(row.config))
        return undefined;
    return row.config;
}
function asString(value, fallback = "") {
    return typeof value === "string" ? value : fallback;
}
function asStringArray(value) {
    return Array.isArray(value) ? value.filter((item) => typeof item === "string") : [];
}
function asNumber(value, fallback) {
    return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}
function asBoolean(value, fallback) {
    return typeof value === "boolean" ? value : fallback;
}
function asStringMap(value) {
    if (value === null || typeof value !== "object" || Array.isArray(value))
        return {};
    return Object.fromEntries(Object.entries(value).filter(([, item]) => typeof item === "string"));
}
/** patch 行中某 transport 的 secret 引用（key → ref）。 */
export function secretRefsFromPatchRow(row) {
    const config = configFromPatchRow(row) ?? {};
    return asStringMap(config[keyValueFields(config.transport).secret]);
}
/** patch config → 行列表；secret 行的 value 为空（值在凭据存储中）。 */
function keyValueRowsFromConfig(config) {
    const fields = keyValueFields(config.transport);
    const plainRows = Object.entries(asStringMap(config[fields.plain])).map(([key, value]) => ({ key, value, secret: false }));
    const secretRows = Object.keys(asStringMap(config[fields.secret])).map((key) => ({ key, value: "", secret: true }));
    return [...plainRows, ...secretRows];
}
/** patch 行 → view。secret 值不返回。 */
export function patchRowToView(row) {
    const config = configFromPatchRow(row);
    if (config === undefined)
        return undefined;
    const serverName = asString(config.serverName);
    if (!SERVER_NAME_RE.test(serverName))
        return undefined;
    const transport = config.transport === "streamable-http" ? "streamable-http" : config.transport === "stdio" ? "stdio" : "unknown";
    const reconnectRaw = config.reconnect !== null && typeof config.reconnect === "object" && !Array.isArray(config.reconnect) ? config.reconnect : {};
    const viewRows = () => keyValueRowsFromConfig(config).map((item) => (item.secret ? { key: item.key, secret: true } : item));
    return {
        serverName,
        transport,
        enabled: row.disabled !== true,
        entryId: row.id,
        command: transport === "stdio" ? asString(config.command) : undefined,
        args: transport === "stdio" ? asStringArray(config.args) : undefined,
        env: transport === "stdio" ? viewRows() : [],
        cwd: transport === "stdio" ? asString(config.cwd) : undefined,
        url: transport === "streamable-http" ? asString(config.url) : undefined,
        headers: transport === "streamable-http" ? viewRows() : [],
        toolCallTimeoutMs: asNumber(config.toolCallTimeoutMs, DEFAULT_TOOL_CALL_TIMEOUT_MS),
        failOnStartupError: asBoolean(config.failOnStartupError, false),
        reconnect: {
            enabled: asBoolean(reconnectRaw.enabled, DEFAULT_RECONNECT.enabled),
            initialDelayMs: asNumber(reconnectRaw.initialDelayMs, DEFAULT_RECONNECT.initialDelayMs),
            maxDelayMs: asNumber(reconnectRaw.maxDelayMs, DEFAULT_RECONNECT.maxDelayMs),
            maxAttempts: asNumber(reconnectRaw.maxAttempts, DEFAULT_RECONNECT.maxAttempts)
        }
    };
}
/** 在受管 + 外部行之间检测重复 serverName。返回重复名单。 */
export function duplicateServerNames(managedRows, externalRows) {
    const names = new Map();
    const add = (rows, owner) => {
        for (const row of rows) {
            const name = asString(row.config?.serverName);
            if (!SERVER_NAME_RE.test(name))
                continue;
            const list = names.get(name) ?? [];
            list.push(owner);
            names.set(name, list);
        }
    };
    add(managedRows, "managed");
    add(externalRows, "external");
    return [...names.entries()].filter(([, owners]) => owners.length > 1).map(([name]) => name);
}
/** 从 patch 行读取完整输入（含明文值，仅供本机 test/编辑使用，不跨 RPC）。secret 行 value 为空。 */
export function inputFromPatchRow(row) {
    const config = configFromPatchRow(row) ?? {};
    const serverName = asString(config.serverName, serverNameFromRowId(row.id) ?? "");
    const common = {
        serverName,
        toolCallTimeoutMs: asNumber(config.toolCallTimeoutMs, DEFAULT_TOOL_CALL_TIMEOUT_MS),
        failOnStartupError: asBoolean(config.failOnStartupError, false),
        reconnect: {
            enabled: asBoolean(config.reconnect?.enabled, DEFAULT_RECONNECT.enabled),
            initialDelayMs: asNumber(config.reconnect?.initialDelayMs, DEFAULT_RECONNECT.initialDelayMs),
            maxDelayMs: asNumber(config.reconnect?.maxDelayMs, DEFAULT_RECONNECT.maxDelayMs),
            maxAttempts: asNumber(config.reconnect?.maxAttempts, DEFAULT_RECONNECT.maxAttempts)
        }
    };
    if (config.transport === "streamable-http") {
        return mcpServerInputSchema.parse({
            ...common,
            transport: "streamable-http",
            url: asString(config.url),
            headers: keyValueRowsFromConfig(config)
        });
    }
    return mcpServerInputSchema.parse({
        ...common,
        transport: "stdio",
        command: asString(config.command),
        args: asStringArray(config.args),
        env: keyValueRowsFromConfig(config),
        cwd: asString(config.cwd)
    });
}
/** 输入中某 transport 的行列表。 */
export function keyValueRowsOf(input) {
    return input.transport === "stdio" ? input.env : input.headers;
}
