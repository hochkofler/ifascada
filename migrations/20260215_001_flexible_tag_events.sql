-- 1. Add raw_tag_id column
ALTER TABLE tag_events ADD COLUMN raw_tag_id TEXT;

-- 2. Populate raw_tag_id with existing data
UPDATE tag_events SET raw_tag_id = tag_id;

-- 3. Make raw_tag_id mandatory (always present in telemetry)
ALTER TABLE tag_events ALTER COLUMN raw_tag_id SET NOT NULL;

-- 4. Make tag_id nullable (for unregistered tags)
ALTER TABLE tag_events ALTER COLUMN tag_id DROP NOT NULL;

-- 5. Create index for raw_tag_id for searching unregistered data
CREATE INDEX idx_tag_events_raw_tag_id ON tag_events(raw_tag_id);
