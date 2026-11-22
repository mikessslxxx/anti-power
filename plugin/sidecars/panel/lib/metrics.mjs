// Metrics for the selected conversation, shared by the panel and CLI
import { sessionContext } from '../vendor/usage-monitor/session-context.mjs';
import { speedMetric } from '../vendor/usage-monitor/trajectory-metrics.mjs';

export function sessionMetrics(rows) {
  const latest = rows.findLast((row) => !row.isCompactionStep);
  return {
    speed: latest ? speedMetric(latest).value : null,
    ttft: latest?.ttftSec ?? null,
    context: sessionContext(rows),
  };
}
