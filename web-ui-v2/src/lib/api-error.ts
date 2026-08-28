export class ApiError extends Error {
  // Campos explicitos en vez de parameter properties (`public readonly status: number`) del
  // original: `erasableSyntaxOnly` de este proyecto no permite esa sintaxis, porque no se borra
  // con solo quitar los tipos. Misma superficie publica.
  readonly status: number;
  readonly body: string;

  constructor(status: number, body: string, options?: { cause?: unknown }) {
    super(`API ${status}: ${body.substring(0, 512)}`, options);
    this.status = status;
    this.body = body;
    this.name = "ApiError";
    // Necesario para que instanceof ApiError funcione correctamente
    // en código transpilado por TypeScript/Babel
    Object.setPrototypeOf(this, ApiError.prototype);
  }

  get isUnauthorized(): boolean {
    return this.status === 401;
  }
  get isForbidden(): boolean {
    return this.status === 403;
  }
  get isNotFound(): boolean {
    return this.status === 404;
  }
  get isServerError(): boolean {
    return this.status >= 500;
  }
  /** true para timeouts (408) o errores de servidor (5xx): candidatos a reintento. */
  get isRetriable(): boolean {
    return this.status === 408 || this.status >= 500;
  }

  /**
   * Mensaje legible para mostrar al usuario. Si el cuerpo es JSON con un campo
   * `message` (string o string[]) lo extrae; si no, devuelve el texto crudo (acotado).
   * El backend de ifascada (Rust) hoy no emite ese shape, asi que cae al texto crudo:
   * es el comportamiento correcto, no un fallback degradado.
   */
  get userMessage(): string {
    try {
      const parsed: unknown = JSON.parse(this.body);
      if (parsed && typeof parsed === "object" && "message" in parsed) {
        const message = parsed.message;
        if (typeof message === "string") return message;
        if (Array.isArray(message)) {
          return message.filter((m): m is string => typeof m === "string").join(". ");
        }
      }
    } catch {
      // body no es JSON: caer al texto crudo
    }
    return this.body.substring(0, 512) || `Error ${this.status}`;
  }
}
