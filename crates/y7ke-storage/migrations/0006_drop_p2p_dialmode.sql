-- The P2p dial mode was removed (it was a behavioural duplicate of
-- Internet — same dial chain; DCUtR runs automatically regardless of
-- mode). Rewrite any settings row still holding the old value so it
-- decodes cleanly as Internet.
UPDATE settings
SET payload_json = json_set(payload_json, '$.dial_mode', 'Internet')
WHERE id = 1 AND json_extract(payload_json, '$.dial_mode') = 'P2p';
