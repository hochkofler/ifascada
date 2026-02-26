ALTER TABLE tags
    DROP CONSTRAINT IF EXISTS ck_tags_tag_code_canonical_format;

ALTER TABLE tags
    ADD CONSTRAINT ck_tags_tag_code_canonical_format
    CHECK (
        tag_code_canonical ~ '^[A-Z0-9_]{2,12}\.[A-Z0-9_]{2,12}\.[A-Z0-9_]{2,12}\.[A-Z0-9_]{2,16}\.[A-Z0-9_]{2,8}\.[A-Z0-9_]{2,8}$'
    );
