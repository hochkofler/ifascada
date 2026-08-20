import { beforeAll, describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import { DataTable } from "./DataTable";
import { ColumnDisplayType } from "./types";
import type { ColumnDefinition, ServerHandlers, ServerState } from "./types";

// The app doesn't wire up i18next globally yet (out of scope for vendoring the DataTable
// system); ./i18n.ts, imported transitively by DataTable.tsx, only *registers* the "tables"
// locale bundle once an i18next instance exists. Without any init, react-i18next's `useTranslation`
// falls back to rendering translation keys verbatim -- init it here so the chrome renders the
// real Spanish strings this component ships with, exercising the vendored i18n wiring too.
beforeAll(async () => {
  await i18n.use(initReactI18next).init({ lng: "es", resources: {} });
});

// `DataTable`'s real props (see ./types.ts DataTableProps) differ from the brief's illustrative
// sketch: it does not take a pre-built TanStack `table` instance. It takes the raw `data` +
// `columns` (as `ColumnDefinition<T>[]`, this lib's own column config -- not TanStack's
// `ColumnDef`) plus the server-driven pagination/sorting/filtering state, and builds the
// TanStack table instance itself internally via `useDataTableInstance`.
type Row = { id: string; value: number };

const columns: ColumnDefinition<Row>[] = [
  { accessorKey: "id", header: "ID", type: ColumnDisplayType.String },
  { accessorKey: "value", header: "Value", type: ColumnDisplayType.Number },
];

const serverState: ServerState = {
  page: 0,
  pageSize: 25,
  sorting: [],
  filters: [],
  globalFilter: "",
};

const noop = () => {
  /* not exercised by this test */
};
const serverHandlers: ServerHandlers = {
  onPageChange: noop,
  onPageSizeChange: noop,
  onSortingChange: noop,
  onFiltersChange: noop,
  onGlobalFilterChange: noop,
};

describe("DataTable (vendored from @ifahub/tables)", () => {
  it("renders rows from the provided data", () => {
    render(
      <DataTable
        data={[
          { id: "a", value: 1 },
          { id: "b", value: 2 },
        ]}
        columns={columns}
        totalRows={2}
        serverState={serverState}
        serverHandlers={serverHandlers}
      />
    );

    expect(screen.getByText("a")).toBeInTheDocument();
    expect(screen.getByText("b")).toBeInTheDocument();
    // Number column goes through formatCellValue/formatQuantity, not raw String(value).
    expect(screen.getByText("1")).toBeInTheDocument();
    expect(screen.getByText("2")).toBeInTheDocument();
  });

  it("renders the empty state when there are no rows", () => {
    render(
      <DataTable
        data={[]}
        columns={columns}
        totalRows={0}
        serverState={serverState}
        serverHandlers={serverHandlers}
      />
    );

    expect(screen.getByText("No se encontraron registros")).toBeInTheDocument();
  });
});
