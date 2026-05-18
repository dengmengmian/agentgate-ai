export interface McpServer {
  name: string;
  command: string;
  args: string[];
  timeout: number | null;
  env: Record<string, string> | null;
  enabled: boolean;
}

export interface McpSource {
  client: string;
  config_path: string;
  servers: McpServer[];
}

export interface McpOverview {
  sources: McpSource[];
  total_servers: number;
  total_clients: number;
}
