import type { JSX } from "react";
import { Search } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Input } from "@/components/ui/input";
import { useDebouncedCallback } from "./hooks/use-debounced-callback";
import { TABLES_NS } from "./i18n";

interface DataTableSearchProps {
  value: string;
  onChange: (value: string) => void;
  debounceMs?: number;
  placeholder?: string;
}

export function DataTableSearch({
  value,
  onChange,
  debounceMs = 300,
  placeholder,
}: DataTableSearchProps): JSX.Element {
  const { t } = useTranslation(TABLES_NS);
  // Uncontrolled: the DOM owns the text while typing and the commit is debounced.
  // `defaultValue` seeds it from the current value (e.g. restored from the URL on mount).
  // The global filter is never reset programmatically after mount, so no external
  // sync is needed — which removes the prop→state mirror this component used to carry.
  const debouncedOnChange = useDebouncedCallback(onChange, debounceMs);

  return (
    <div className="relative">
      <Search className="absolute left-2.5 top-2.5 size-4 text-muted-foreground" />
      <Input
        placeholder={placeholder ?? t("search.placeholder")}
        defaultValue={value}
        onChange={(e) => {
          debouncedOnChange(e.target.value);
        }}
        className="pl-8 h-9 w-64"
      />
    </div>
  );
}
