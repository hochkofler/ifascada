#!/bin/sh
set -eu

run_sql() {
  file="$1"
  echo "Applying ${file} ..."
  psql "${PG_DSN}" -v ON_ERROR_STOP=1 -f "${file}"
}

for file in \
  /migrations/0001_core_postgres.sql \
  /migrations/0003_tag_naming_governance.sql \
  /migrations/0005_fix_tag_naming_constraint_regex.sql \
  /migrations/0006_context_hierarchy.sql \
  /migrations/0009_operational_events.sql \
  /migrations/0010_connection_domain_state.sql \
  /migrations/0011_device_domain_state.sql \
  /migrations/0012_edges_metadata_json.sql \
  /migrations/0016_telemetry_received_at.sql \
  /migrations/0020_edge_control_command.sql \
  /migrations/0021_mark_action_executions_obsolete.sql; do
  run_sql "${file}"
done

if [ "$(psql "${PG_DSN}" -tA -c "SELECT EXISTS (SELECT 1 FROM pg_available_extensions WHERE name = 'timescaledb');" | tr -d '\r')" = "t" ]; then
  run_sql /migrations/0002_timescale_historian.sql
else
  echo "Skipping /migrations/0002_timescale_historian.sql (timescaledb extension not available)."
fi

case "${SEED_PROFILE}" in
  minimal)
    seed_files="/migrations/0015_dev_seed_minimal_three_edges.sql /migrations/0017_printer_device_command_and_negative_trigger.sql"
    ;;
  sim20)
    seed_files="/migrations/0004_dev_seed_minimal_catalog.sql /migrations/0007_dev_seed_context_hierarchy.sql /migrations/0008_dev_seed_sim20_multi_area.sql"
    ;;
  full)
    seed_files="/migrations/0004_dev_seed_minimal_catalog.sql /migrations/0007_dev_seed_context_hierarchy.sql /migrations/0008_dev_seed_sim20_multi_area.sql /migrations/0013_scale_manual_config_in_catalog.sql /migrations/0014_dev_seed_modbus_rtu_com10_multi_slave.sql /migrations/0017_printer_device_command_and_negative_trigger.sql"
    ;;
  *)
    echo "Invalid SEED_PROFILE=${SEED_PROFILE}. Use minimal|sim20|full."
    exit 1
    ;;
esac

for file in ${seed_files}; do
  run_sql "${file}"
done

echo "Done. Seed profile: ${SEED_PROFILE}"
