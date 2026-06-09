#!/usr/bin/env bash
# Dogfood:实时观察 Codex remote compaction 在网关上的触发情况。
#
# 每 5 秒查一次 SQLite 日志,展示最近 1 小时内 trace.mode = "codex_compact"
# 的请求(时间、provider、模型、延迟、上游 summary 长度估计)。
#
# 用法:bash scripts/codex_compact_tail.sh
#
# 退出:Ctrl-C

set -euo pipefail

DB="${AGENTGATE_DB:-$HOME/Library/Application Support/com.mengmian.agentgate/agentgate.db}"
if [[ ! -f "$DB" ]]; then
  echo "找不到 DB:$DB"
  echo "设 AGENTGATE_DB 指向你的实际 DB 路径"
  exit 1
fi

INTERVAL="${1:-5}"
WINDOW_MIN="${2:-60}"

printf '\n监听 codex_compact 触发(每 %ss 刷新,窗口 %s 分钟)\nDB: %s\n按 Ctrl-C 退出\n\n' \
  "$INTERVAL" "$WINDOW_MIN" "$DB"

last_id=""
while true; do
  rows=$(sqlite3 "$DB" <<SQL
.mode list
.separator |
SELECT
  id,
  substr(timestamp, 12, 8) AS hhmmss,
  COALESCE(provider, '?'),
  COALESCE(model, '?'),
  latency_ms,
  COALESCE(input_tokens, 0),
  COALESCE(output_tokens, 0)
FROM request_logs
WHERE source = 'gateway'
  AND trace_json LIKE '%"mode":"codex_compact"%'
  AND datetime(timestamp) >= datetime('now', '-${WINDOW_MIN} minutes')
ORDER BY timestamp DESC
LIMIT 20;
SQL
)

  if [[ -n "$rows" ]]; then
    new_id=$(printf '%s\n' "$rows" | head -1 | cut -d'|' -f1)
    if [[ "$new_id" != "$last_id" ]]; then
      clear
      printf '%-10s  %-22s  %-18s  %6s  %7s  %7s\n' \
        "TIME" "PROVIDER" "MODEL" "LAT_MS" "IN_TOK" "OUT_TOK"
      printf '%s\n' "$rows" | while IFS='|' read -r id hhmmss provider model latency in_tok out_tok; do
        printf '%-10s  %-22s  %-18s  %6s  %7s  %7s\n' \
          "$hhmmss" "$provider" "$model" "$latency" "$in_tok" "$out_tok"
      done
      total=$(sqlite3 "$DB" \
        "SELECT COUNT(*) FROM request_logs WHERE source='gateway' AND trace_json LIKE '%\"mode\":\"codex_compact\"%' AND timestamp LIKE strftime('%Y-%m-%d', 'now') || '%';")
      printf '\n今日累计:%s 次\n' "$total"
      last_id="$new_id"
    fi
  else
    if [[ -z "$last_id" ]]; then
      printf '\r等待触发... (%s)' "$(date +%H:%M:%S)"
    fi
  fi
  sleep "$INTERVAL"
done
