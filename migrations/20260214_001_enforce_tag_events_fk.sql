-- 1. Clean up orphaned data that would violate the new constraint
DELETE FROM tag_events WHERE tag_id NOT IN (SELECT id FROM tags);

-- 2. Add Foreign Key Constraint
ALTER TABLE tag_events
ADD CONSTRAINT fk_tag_events_tag
FOREIGN KEY (tag_id) REFERENCES tags(id)
ON DELETE CASCADE;
