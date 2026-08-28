/**
 * Copia de libs/auth/src/permissions.ts de ifahub, reducida a lo que `nav.ts` importa.
 *
 * Vale aclarar por que esto NO arrastra Authentik: `hasPermission` no sabe nada de OIDC ni de
 * tokens -- recibe un `readonly string[]` y hace `.includes()`. Todo lo especifico de Authentik
 * vive en `parseEntitlements`, que `nav.ts` no usa. El acoplamiento era aparente.
 *
 * Esta app todavia no tiene login (ver `useCan` en @/lib/use-can, que devuelve `true`), asi que
 * el chrome le pasa `ALL_PERMISSIONS` y el comodin deja pasar todo. El dia que haya identidad,
 * el unico cambio es de donde salen los entitlements: la logica ya esta y los `vistaCode` del
 * nav ya estan cableados.
 */
export type PermissionCode = string;

/** Comodin de acceso total. Lo que usa el chrome mientras no exista autenticacion. */
export const ALL_PERMISSIONS: readonly string[] = ["*"];

/**
 * True si el usuario tiene el permiso dado.
 * Acepta un codigo suelto, o un array (OR: alcanza con tener uno).
 */
export function hasPermission(
  entitlements: readonly string[],
  code: PermissionCode | readonly PermissionCode[]
): boolean {
  // Comodin: "*" = acceso total (superadmin). Habilita cualquier codigo, incluso futuros.
  if (entitlements.includes("*")) return true;
  if (Array.isArray(code)) {
    return (code as readonly PermissionCode[]).some((c) => entitlements.includes(c));
  }
  return entitlements.includes(code as PermissionCode);
}
