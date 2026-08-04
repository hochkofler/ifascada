CREATE TABLE IF NOT EXISTS lines (
    id BIGSERIAL PRIMARY KEY,
    site_id BIGINT NOT NULL REFERENCES sites(id),
    code TEXT NOT NULL,
    name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (site_id, code)
);

CREATE TABLE IF NOT EXISTS areas (
    id BIGSERIAL PRIMARY KEY,
    line_id BIGINT NOT NULL REFERENCES lines(id),
    code TEXT NOT NULL,
    name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (line_id, code)
);

CREATE TABLE IF NOT EXISTS cells (
    id BIGSERIAL PRIMARY KEY,
    area_id BIGINT NOT NULL REFERENCES areas(id),
    code TEXT NOT NULL,
    name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (area_id, code)
);

ALTER TABLE edges
    ADD COLUMN IF NOT EXISTS cell_id BIGINT NULL REFERENCES cells(id);

CREATE INDEX IF NOT EXISTS idx_lines_site_id ON lines(site_id);
CREATE INDEX IF NOT EXISTS idx_areas_line_id ON areas(line_id);
CREATE INDEX IF NOT EXISTS idx_cells_area_id ON cells(area_id);
CREATE INDEX IF NOT EXISTS idx_edges_cell_id ON edges(cell_id);
