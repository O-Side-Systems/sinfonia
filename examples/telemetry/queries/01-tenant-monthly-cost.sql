-- Plan §8.2 reference dashboard query #1.
--
-- Q: "What did this tenant cost us this month?"
--
-- Reads from the `events` table where `bridge.cost_update` spans land.
-- `cost_delta_usd` is the stringified Decimal the bridge wrote (per
-- STATUS §5.1, monetary values are never f64 on the wire), so we cast
-- to numeric before summing.
SELECT
    SUM((attributes->>'cost_delta_usd')::numeric) AS total_usd
FROM events
WHERE tenant_id = 'kyros-web-app'
  AND span_name = 'bridge.cost_update'
  AND started_at >= date_trunc('month', NOW());
