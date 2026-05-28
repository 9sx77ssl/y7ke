-- Replace the 4-bool dial_modes with a single enum string field.
-- Existing rows likely have dial_modes=*; "Internet" is the safe default
-- because the old default had {lan:true,internet:true,relay:true,p2p:false}
-- which behaviourally maps to Internet.
UPDATE settings
SET payload_json = json_set(
    json_remove(payload_json, '$.dial_modes'),
    '$.dial_mode',
    'Internet'
)
WHERE id = 1;
