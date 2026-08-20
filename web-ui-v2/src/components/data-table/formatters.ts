import {
  ColumnDisplayType,
  getNumberLocale,
  formatQuantity,
  formatAmount,
  formatDate,
  formatDateTime,
} from "./types";

/** Localized labels for a Boolean cell — resolved by the component layer (i18n) and threaded in,
 *  since this formatter is pure and also runs outside React (CSV export). */
export interface BooleanCellLabels {
  true: string;
  false: string;
}

/**
 * Convierte un valor de celda a string de forma segura. Solo primitivos producen
 * texto; objetos/funciones/símbolos devuelven "" en vez de "[object Object]".
 */
function toDisplayString(value: unknown): string {
  if (value === null || value === undefined) return "";
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean" || typeof value === "bigint") {
    return String(value);
  }
  return "";
}

function toDateInput(value: unknown): string | number | Date | null {
  if (typeof value === "string" || typeof value === "number" || value instanceof Date) {
    return value;
  }
  return null;
}

export function formatCellValue(
  value: unknown,
  type: ColumnDisplayType,
  // Retained for signature stability; number/amount formatting is centralized in ./types
  // (FORMAT_LOCALE), so the locale is no longer threaded through the cell formatter.
  locale = getNumberLocale(),
  booleanLabels?: BooleanCellLabels
): string {
  void locale;
  switch (type) {
    case ColumnDisplayType.String:
      return toDisplayString(value);
    case ColumnDisplayType.Number: {
      const n = Number(value);
      return isNaN(n) ? "" : formatQuantity(n);
    }
    case ColumnDisplayType.Currency: {
      // Amount only — the currency (SAP documents are multi-currency) belongs to the data/column
      // config, not the formatter. Previously hardcoded "Bs".
      const n = Number(value);
      return isNaN(n) ? "" : formatAmount(n);
    }
    case ColumnDisplayType.Percentage: {
      const n = Number(value);
      return isNaN(n) ? "" : `${n.toFixed(2)} %`;
    }
    case ColumnDisplayType.Date:
      return formatDate(toDateInput(value));
    case ColumnDisplayType.DateTime:
      return formatDateTime(toDateInput(value));
    case ColumnDisplayType.Boolean:
      return value ? (booleanLabels?.true ?? "") : (booleanLabels?.false ?? "");
    default:
      return toDisplayString(value);
  }
}
