import { useState, type JSX } from "react";
import type { Column } from "@tanstack/react-table";
import { FilterType } from "./types";
import { X } from "lucide-react";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { useDebouncedCallback } from "./hooks/use-debounced-callback";

// Normaliza texto para filtrado: trim, minúsculas y sin diacríticos (tildes, ñ, ü…)
function normalizeFilter(text: string): string {
  return text.trim().toLowerCase().normalize("NFD").replace(/\p{M}/gu, "");
}

function FilterText({ column }: { column: Column<object> }) {
  // Mounted only while the column filter is open, so the initial value reflects the current
  // filter on each open; the debounced commit lives in the change handler (no mirror effect).
  const [val, setVal] = useState((column.getFilterValue() ?? "") as string);
  const debouncedApply = useDebouncedCallback((next: string) => {
    column.setFilterValue(normalizeFilter(next) || undefined);
  }, 300);

  return (
    <div
      className="relative w-full"
      onClick={(e) => {
        e.stopPropagation();
      }}
    >
      <Input
        type="text"
        value={val}
        onChange={(e) => {
          setVal(e.target.value);
          debouncedApply(e.target.value);
        }}
        placeholder="Filtrar…"
        autoFocus
        className="h-6 text-[11px] pr-6 border-border/40 bg-background/50 placeholder:text-muted-foreground/50 focus-visible:ring-0 focus-visible:ring-offset-0"
      />
      {val && (
        <button
          type="button"
          onClick={() => {
            setVal("");
            column.setFilterValue(undefined);
          }}
          className="absolute right-1.5 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
        >
          <X className="size-2.5" />
        </button>
      )}
    </div>
  );
}

function FilterSelect({
  column,
  options,
}: {
  column: Column<object>;
  options: { label: string; value: string }[];
}) {
  const selected = (column.getFilterValue() as string[] | undefined) ?? [];

  function toggle(val: string) {
    const next = selected.includes(val) ? selected.filter((v) => v !== val) : [...selected, val];
    column.setFilterValue(next.length ? next : undefined);
  }

  return (
    <div
      className="flex flex-col gap-0.5 rounded border border-border/40 bg-background p-1"
      onClick={(e) => {
        e.stopPropagation();
      }}
    >
      {options.map((opt) => (
        <label
          key={opt.value}
          className="flex cursor-pointer items-center gap-1.5 rounded-sm px-1 py-0.5 text-[11px] hover:bg-muted/50"
        >
          <Checkbox
            checked={selected.includes(opt.value)}
            onCheckedChange={() => {
              toggle(opt.value);
            }}
            className="size-3"
          />
          {opt.label}
        </label>
      ))}
    </div>
  );
}

function FilterNumber({ column }: { column: Column<object> }) {
  // Mounted only while the column filter is open; the debounced commit lives in the change handlers.
  const [min, max] = (column.getFilterValue() as [string, string] | undefined) ?? ["", ""];
  const [localMin, setMin] = useState(min);
  const [localMax, setMax] = useState(max);
  const debouncedApply = useDebouncedCallback((lo: string, hi: string) => {
    column.setFilterValue(lo || hi ? [lo, hi] : undefined);
  }, 300);

  return (
    <div
      className="flex items-center gap-1 w-full"
      onClick={(e) => {
        e.stopPropagation();
      }}
    >
      <Input
        type="number"
        value={localMin}
        onChange={(e) => {
          setMin(e.target.value);
          debouncedApply(e.target.value, localMax);
        }}
        placeholder="Min"
        className="h-6 text-[11px] min-w-0 px-1.5 border-border/40 bg-background/50 focus-visible:ring-0 focus-visible:ring-offset-0"
      />
      <span className="shrink-0 text-[10px] text-muted-foreground">—</span>
      <Input
        type="number"
        value={localMax}
        onChange={(e) => {
          setMax(e.target.value);
          debouncedApply(localMin, e.target.value);
        }}
        placeholder="Max"
        className="h-6 text-[11px] min-w-0 px-1.5 border-border/40 bg-background/50 focus-visible:ring-0 focus-visible:ring-offset-0"
      />
      {(localMin || localMax) && (
        <button
          type="button"
          onClick={() => {
            setMin("");
            setMax("");
            column.setFilterValue(undefined);
          }}
          className="shrink-0 text-muted-foreground hover:text-foreground"
        >
          <X className="size-2.5" />
        </button>
      )}
    </div>
  );
}

function FilterDate({ column }: { column: Column<object> }) {
  return (
    <div
      className="w-full"
      onClick={(e) => {
        e.stopPropagation();
      }}
    >
      <Input
        type="date"
        value={(column.getFilterValue() ?? "") as string}
        onChange={(e) => {
          column.setFilterValue(e.target.value || undefined);
        }}
        className="h-6 text-[11px] border-border/40 bg-background/50 focus-visible:ring-0 focus-visible:ring-offset-0"
      />
    </div>
  );
}

export function ColumnFilterInput({ column }: { column: Column<object> }): JSX.Element {
  const meta = column.columnDef.meta;
  const filterType = meta?.filterType ?? FilterType.Text;
  const options = meta?.filterOptions ?? [];

  if (filterType === FilterType.Select) return <FilterSelect column={column} options={options} />;
  if (filterType === FilterType.Number) return <FilterNumber column={column} />;
  if (filterType === FilterType.Date) return <FilterDate column={column} />;
  return <FilterText column={column} />;
}
