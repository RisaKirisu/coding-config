/**
 * dsh-skill-mcp-panel/mcp-client —— 面板受管 MCP 行的运行入口。
 *
 * 配置与 @deepseek-ai/dsh-mcp-client 相同，另加 `secretHeaders` / `secretEnv`
 * （key → 凭据引用）。启动时从 DSH 凭据存储解析这些引用，合并进
 * headers / env 后以子插件启动官方 MCP 客户端；任一引用的存储值变化时重启，
 * 让新值立即生效。引用缺失时启动失败，不会带着缺失的密钥连接。
 */
import * as mcpClient from "@deepseek-ai/dsh-mcp-client";
import { credentialRef } from "@deepseek-ai/dsh-credentials";
export const name = "dsh-skill-mcp-panel-mcp-client";
export const inject = ["credentials"];
async function resolveSecrets(ctx, serverName, secretRefs) {
    const values = {};
    for (const [key, ref] of Object.entries(secretRefs)) {
        const resolved = await ctx.credentials.resolve(credentialRef(ref));
        if (resolved === undefined)
            throw new Error("mcp-client(" + serverName + '): secret "' + key + '" has no stored value for credential reference ' + ref);
        values[key] = resolved.value;
    }
    return values;
}
export async function apply(ctx, config) {
    const { secretHeaders = {}, secretEnv = {}, ...clientConfig } = config;
    const referencedRefs = new Set([...Object.values(secretHeaders), ...Object.values(secretEnv)]);
    ctx.on("credentials/reference-updated", (ref) => {
        if (referencedRefs.has(ref))
            void ctx.fiber.restart();
    });
    const resolvedConfig = clientConfig.transport === "stdio"
        ? { ...clientConfig, env: { ...clientConfig.env, ...(await resolveSecrets(ctx, clientConfig.serverName, secretEnv)) } }
        : { ...clientConfig, headers: { ...clientConfig.headers, ...(await resolveSecrets(ctx, clientConfig.serverName, secretHeaders)) } };
    await ctx.plugin(mcpClient, resolvedConfig);
}
