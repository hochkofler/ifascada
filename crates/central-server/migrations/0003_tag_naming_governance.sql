ALTER TABLE tags
    ADD COLUMN IF NOT EXISTS tag_code_canonical TEXT,
    ADD COLUMN IF NOT EXISTS display_name TEXT,
    ADD COLUMN IF NOT EXISTS aliases_json JSONB NOT NULL DEFAULT '[]'::jsonb;

UPDATE tags
SET tag_code_canonical = tag_code
WHERE tag_code_canonical IS NULL;

UPDATE tags
SET display_name = name
WHERE display_name IS NULL;

ALTER TABLE tags
    ALTER COLUMN tag_code_canonical SET NOT NULL,
    ALTER COLUMN display_name SET NOT NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'ck_tags_tag_code_canonical_format'
          AND conrelid = 'tags'::regclass
    ) THEN
        ALTER TABLE tags
            ADD CONSTRAINT ck_tags_tag_code_canonical_format
            CHECK (
                tag_code_canonical ~ '^[A-Z0-9_]{2,12}\\.[A-Z0-9_]{2,12}\\.[A-Z0-9_]{2,12}\\.[A-Z0-9_]{2,16}\\.[A-Z0-9_]{2,8}\\.[A-Z0-9_]{2,8}$'
            );
    END IF;
END $$;

CREATE UNIQUE INDEX IF NOT EXISTS ux_tags_tag_code_canonical
    ON tags(tag_code_canonical);

CREATE INDEX IF NOT EXISTS idx_tags_display_name
    ON tags(display_name);

CREATE INDEX IF NOT EXISTS idx_tags_aliases_json_gin
    ON tags USING GIN(aliases_json);
