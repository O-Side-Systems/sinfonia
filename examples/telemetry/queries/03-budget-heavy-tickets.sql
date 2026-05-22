-- Plan §8.2 reference dashboard query #3.
--
-- Q: "Which tickets are eating disproportionate budget?"
--
-- Aggregates per-ticket token totals from the `sessions` table.
-- Reports the top 20 by total tokens. Swap `total_tokens` for a cost
-- sum (joining against `bridge.cost_update` events) if you'd rather
-- rank by dollars.
SELECT
    s.issue_id,
    s.issue_ident,
    SUM(s.prompt_tokens + s.completion_tokens)::bigint AS tokens,
    COUNT(*) AS sessions
FROM sessions s
WHERE s.tenant_id = 'kyros-web-app'
GROUP BY s.issue_id, s.issue_ident
ORDER BY tokens DESC
LIMIT 20;
