/**
 * Auth-readiness stub (see docs/superpowers/specs/2026-08-20-web-ui-v2-rewrite-design.md,
 * "Auth: door left open, not implemented"). No login exists yet, so every permission check
 * passes. When real OIDC/Authentik auth is added later, this becomes the single place that
 * changes -- callers (like DataTableSavedViews' permission-gated actions) don't change.
 */
export function useCan(_permission: string): boolean {
  return true;
}
