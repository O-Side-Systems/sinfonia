-- Plan §8.2 reference dashboard query #2.
--
-- Q: "How often does the agent close a ticket without human
--     intervention?"
--
-- Proxy: `attempts.attempt_number = 1 AND outcome = 'green'` over the
-- trailing 30 days. The CI loop increments `attempt_number` for every
-- red retry, so a first-attempt-green is the closest signal we have to
-- "the agent got it right the first time."
SELECT
    COUNT(*) FILTER (WHERE attempt_number = 1)::float
      / NULLIF(COUNT(*), 0) AS first_try_rate,
    COUNT(*) AS total_attempts,
    COUNT(*) FILTER (WHERE attempt_number = 1) AS first_try_greens
FROM attempts
WHERE tenant_id = 'kyros-web-app'
  AND outcome = 'green'
  AND recorded_at >= NOW() - INTERVAL '30 days';
