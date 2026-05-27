-- 0003_dedup_outgoing_requests.sql
--
-- One-shot cleanup for installs that accumulated duplicate outgoing
-- pending requests during the V2-A4 dial-discovery bug: each click of
-- "send request" on a peer that couldn't be reached created a fresh
-- row, resulting in a long list of identical "pending…" cards in the
-- Requests view. Keep the earliest row per peer, drop the rest.
DELETE FROM requests
WHERE direction = 'outgoing'
  AND resolved_at IS NULL
  AND id NOT IN (
    SELECT MIN(id)
    FROM requests
    WHERE direction = 'outgoing' AND resolved_at IS NULL
    GROUP BY peer_y7_id
  );
