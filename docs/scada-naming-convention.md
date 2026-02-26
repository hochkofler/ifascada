# SCADA Naming Convention (Canonical + Operational)

## 1. Why this matters
Consistent naming is operationally critical in SCADA:
1. Faster troubleshooting and operator handoff.
2. Consistent alarm and trend interpretation.
3. Stable integration with reports and external systems.
4. Lower risk of command mistakes.

## 2. Three-name model
Use three identifiers per tag:
1. `tag_id` (technical, immutable)
- Internal stable ID for storage and message routing.

2. `tag_code_canonical` (normative/canonical)
- Governed naming convention for engineering and operations.

3. `display_name` (HMI-friendly)
- Readable label in UI; can evolve without breaking integrations.

Optional:
4. `aliases_json`
- Legacy names and synonyms for migration/search.

## 3. Canonical format
Format:
- `SITE.AREA.UNIT.DEVICE.SIGNAL.ATTRIBUTE`

Regex:
- `^[A-Z0-9_]{2,12}\.[A-Z0-9_]{2,12}\.[A-Z0-9_]{2,12}\.[A-Z0-9_]{2,16}\.[A-Z0-9_]{2,8}\.[A-Z0-9_]{2,8}$`

Segment semantics:
1. `SITE`: plant/site code.
2. `AREA`: process area.
3. `UNIT`: production unit/line.
4. `DEVICE`: instrument/equipment identifier.
5. `SIGNAL`: measured/controlled variable family.
6. `ATTRIBUTE`: semantic attribute (`PV`, `SP`, `OP`, `ALM`, etc).

## 4. Valid examples
1. `PLANTA1.CALDERA.U01.PT101.PRES.PV`
2. `PLANTA1.ENVASADO.L02.MTR301.SPEED.PV`
3. `SITEA.AREA01.UNIT2.FT203.FLOW.SP`

## 5. Invalid examples
1. `planta1.CALDERA.U01.PT101.PRES.PV`
- invalid: lowercase.

2. `PLANTA1.CALDERA.U01.PT 101.PRES.PV`
- invalid: spaces.

3. `PLANTA1.CALDERA.U01.PT101.PRES`
- invalid: missing segment.

4. `PLANTA1.CALDERA.U01.PT101.PRES.PV.EXTRA`
- invalid: extra segment.

## 6. Governance rules
1. `tag_id` never changes.
2. `tag_code_canonical` must be unique globally (or by tenant scope if needed).
3. `display_name` can change through controlled config versioning.
4. Renames update `aliases_json` for backward search compatibility.
5. Command endpoints should accept `tag_id`; UI may resolve by canonical/display.

## 7. Current implementation in central-server
1. Code validator:
- `crates/central-server/src/naming.rs`

2. Schema governance migration:
- `crates/central-server/migrations/0003_tag_naming_governance.sql`
