import { MutationCache, QueryCache, QueryClient } from "@tanstack/react-query";
import { ApiError } from "@/lib/api-error";
import { notify } from "@/components/notifications";

/**
 * Cosechado de apps/ifa-web/src/lib/query-client.ts de ifahub, con una omision deliberada:
 * NO se traen sus `staleTime: 10min` / `gcTime: 30min`. Esos valores son correctos para SAP B1
 * ("cambia lentamente"), y serian activamente erroneos aca: las pantallas de esta app sondean
 * cada 2500 ms via `refetchInterval` y ademas reciben nudges por SSE. Se dejan los defaults de
 * TanStack Query (staleTime 0), que es lo que el codigo existente ya asume.
 *
 * Lo que si se trae es el manejo de errores, que es lo que faltaba: hasta ahora esta app
 * construia `new QueryClient()` pelado y todo fallo de query se perdia en silencio.
 */

const MAX_RETRIES = 3;

/** Mensaje legible a partir de un error desconocido (para diagnostico). */
function errorMessage(error: unknown): string {
  if (error instanceof ApiError) return error.userMessage;
  return "Ocurrio un error inesperado.";
}

/** Readable "source" for the session log from a TanStack Query/mutation key (arbitrary JSON). */
function keySource(key: readonly unknown[] | undefined): string | undefined {
  const parts = key?.filter((part): part is string => typeof part === "string") ?? [];
  return parts.length > 0 ? parts.join(".") : undefined;
}

/**
 * Reintenta solo errores transitorios: timeout (408), errores de servidor (5xx) y fallos de red
 * (rechazo de fetch que no es ApiError). NUNCA reintenta 4xx deterministas: reintentarlos solo
 * anade latencia y carga.
 */
function shouldRetry(failureCount: number, error: unknown): boolean {
  if (failureCount >= MAX_RETRIES) return false;
  if (error instanceof ApiError) return error.isRetriable;
  // No es ApiError -> error de red / fetch rechazado -> transitorio.
  return true;
}

export const queryClient = new QueryClient({
  // Las queries muestran su propio estado de error inline (sin toast ruidoso), pero TODO error
  // igual queda registrado en el panel de mensajes (campana), para que el operador pueda copiarlo
  // y delegarlo aunque nadie haya cableado un notify puntual.
  queryCache: new QueryCache({
    onError: (error, query) => {
      console.error("[query]", errorMessage(error), error);
      notify.logApiError(error, {
        titleKey: "notifications:log.autoTitle.query",
        source: keySource(query.queryKey),
      });
    },
  }),
  // Las mutaciones (acciones del operador) muestran su feedback con un toast especifico desde el
  // componente; aca ademas se garantiza que TODA mutacion fallida quede en el panel aunque el
  // componente no haya llamado a notify.apiError (notify.logApiError se desactiva sola si ya se
  // reporto -- ver notify.ts).
  mutationCache: new MutationCache({
    onError: (error, _variables, _context, mutation) => {
      console.error("[mutation]", errorMessage(error), error);
      notify.logApiError(error, {
        titleKey: "notifications:log.autoTitle.mutation",
        source: keySource(mutation.options.mutationKey),
      });
    },
  }),
  defaultOptions: {
    queries: {
      retry: shouldRetry,
      retryDelay: (attemptIndex) => Math.min(1000 * 2 ** attemptIndex, 30000),
    },
    mutations: {
      // Las acciones de edge (reset, restart) no son idempotentes: un reintento ciego le manda
      // dos veces el comando a un equipo de planta.
      retry: 0,
    },
  },
});
