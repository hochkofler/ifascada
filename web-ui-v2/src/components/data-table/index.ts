import "./i18n";

// Public surface: what consumers of `@/components/data-table` import. Internal composition
// (components importing each other, e.g. DataTableContent's context) stays relative.
export { DataTable } from "./DataTable";
export { DataTableRoot } from "./DataTableRoot";
export { DataTableContent, DataTableContext, useDataTableContext } from "./DataTableContent";
export { DataTableToolbar, DataTableFilterChip } from "./DataTableToolbar";
export { DataTableSearch } from "./DataTableSearch";
export { DataTableColumnsDialog } from "./DataTableColumnsDialog";
export { DataTablePagination } from "./DataTablePagination";
export { DataTableSavedViews } from "./DataTableSavedViews";
export { DataTableEmpty } from "./DataTableEmpty";
export { DataTableError } from "./DataTableError";
export { DataTableLoading } from "./DataTableLoading";
export { DataTableViewOptions } from "./DataTableViewOptions";
export { DetailLinkCell } from "./DetailLinkCell";
export { DocumentRouteProvider, useDocumentRoute } from "./context/document-route-context";
export { validateTableSearch } from "./utils/tableSearch";
export { ColumnDisplayType, FilterType } from "./types";

export type {
  ColumnDefinition,
  ServerState,
  ServerHandlers,
  DataTableProps,
  DataTableRootProps,
  TableDensity,
  TableLayout,
  EmptyStateConfig,
  FilterOption,
  UseDataTableOptions,
  DocumentRef,
} from "./types";
export type { DocumentRouteTarget, DocumentRouteResolver } from "./context/document-route-context";
