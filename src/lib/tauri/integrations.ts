import { invoke } from "@tauri-apps/api/core";
import type { McpStatus } from "@/types/vault";

export function startMcpServer(port?: number): Promise<McpStatus> {
  return invoke<McpStatus>("start_mcp_server", { port: port ?? null });
}

export function stopMcpServer(): Promise<McpStatus> {
  return invoke<McpStatus>("stop_mcp_server");
}

export function mcpStatus(): Promise<McpStatus> {
  return invoke<McpStatus>("mcp_status");
}
