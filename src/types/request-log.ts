export interface RequestLogListItem {
  id: string;
  request_id: string;
  timestamp: string;
  client: string | null;
  provider: string | null;
  model: string | null;
  route: string | null;
  status_code: number | null;
  latency_ms: number | null;
  error_message: string | null;
}

export interface RequestLogDetail {
  id: string;
  request_id: string;
  timestamp: string;
  client: string | null;
  provider: string | null;
  model: string | null;
  route: string | null;
  status_code: number | null;
  latency_ms: number | null;
  input_tokens: number | null;
  output_tokens: number | null;
  raw_request: string | null;
  converted_request: string | null;
  raw_response: string | null;
  converted_response: string | null;
  sse_events: string | null;
  tool_calls: string | null;
  error_message: string | null;
  trace_json: string | null;
}

export interface RequestLogFilter {
  client?: string;
  provider?: string;
  status?: string;
  keyword?: string;
  limit?: number;
  offset?: number;
}
