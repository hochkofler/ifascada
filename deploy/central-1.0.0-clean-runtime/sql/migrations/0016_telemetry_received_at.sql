ALTER TABLE telemetry_ingest_events
    ADD COLUMN IF NOT EXISTS received_at TIMESTAMPTZ;

UPDATE telemetry_ingest_events
SET received_at = COALESCE(received_at, NOW())
WHERE received_at IS NULL;

ALTER TABLE telemetry_ingest_events
    ALTER COLUMN received_at SET DEFAULT NOW();

ALTER TABLE telemetry_ingest_events
    ALTER COLUMN received_at SET NOT NULL;

DO $$
BEGIN
    IF to_regclass('public.telemetry_samples') IS NOT NULL THEN
        ALTER TABLE telemetry_samples
            ADD COLUMN IF NOT EXISTS received_at TIMESTAMPTZ;

        UPDATE telemetry_samples
        SET received_at = COALESCE(received_at, NOW())
        WHERE received_at IS NULL;

        ALTER TABLE telemetry_samples
            ALTER COLUMN received_at SET DEFAULT NOW();

        ALTER TABLE telemetry_samples
            ALTER COLUMN received_at SET NOT NULL;
    END IF;
END $$;
