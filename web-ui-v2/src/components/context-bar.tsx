import { useEffect, useRef } from "react";
import { useQuery } from "@tanstack/react-query";
import { fetchTagsCurrent, fetchLines, fetchAreas, fetchCells, fetchEdgesCurrent } from "@/lib/api-client";
import { useOperationalContextStore } from "@/store/context-store";
import { Select, SelectTrigger, SelectValue, SelectContent, SelectItem } from "@/components/ui/select";
import { Button } from "@/components/ui/button";
import { useAutoSelectFirst } from "@/lib/use-auto-select-first";
import { useTranslation } from "react-i18next";

export function ContextBar() {
  const { t } = useTranslation();
  const { site, line, area, cell, edge, setSite, setLine, setArea, setCell, setEdge } = useOperationalContextStore();

  // No dedicated "list of sites" endpoint exists (verified against api.rs). Deriving the real,
  // currently-reporting site list from tag data is what actually fixes "Site is fixed text".
  const allTags = useQuery({ queryKey: ["all-sites-probe"], queryFn: () => fetchTagsCurrent(1000) });
  const sites = Array.from(new Set((allTags.data ?? []).map((tg) => tg.site_code))).sort();
  useAutoSelectFirst(sites, site, setSite);

  const linesQuery = useQuery({ queryKey: ["ctxb-lines", site], queryFn: () => fetchLines(site) });
  const areasQuery = useQuery({ queryKey: ["ctxb-areas", site, line], queryFn: () => fetchAreas(site, line || undefined) });
  const cellsQuery = useQuery({ queryKey: ["ctxb-cells", site, line, area], queryFn: () => fetchCells(site, line || undefined, area || undefined) });
  const edgesQuery = useQuery({
    queryKey: ["ctxb-edges", site, line, area, cell],
    queryFn: () => fetchEdgesCurrent(200, { site, line: line || undefined, area: area || undefined, cell: cell || undefined }),
  });
  const edgeOptions = Array.from(new Set((edgesQuery.data ?? []).map((e) => e.edge_code))).sort();

  // web-ui's own ContextBar does NOT clear child selections when a parent changes (e.g.
  // picking a different Line while an Area from the old Line is still selected leaves a
  // stale, now-invalid Area value in the store) -- this fixes that gap rather than porting it.
  //
  // Each effect below only clears children when its own level's value actually *changes*
  // after mount (tracked via a ref of the previous value), not on the initial render: a plain
  // `useEffect(..., [site])` also fires once right after mount regardless of whether `site`
  // changed, which would wipe out a line/area/cell/edge selection that was already present in
  // the (module-level, cross-navigation) store the moment this component first renders.
  const prevSite = useRef(site);
  useEffect(() => {
    if (prevSite.current !== site) {
      setLine("");
      setArea("");
      setCell("");
      setEdge("");
    }
    prevSite.current = site;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [site]);
  const prevLine = useRef(line);
  useEffect(() => {
    if (prevLine.current !== line) {
      setArea("");
      setCell("");
      setEdge("");
    }
    prevLine.current = line;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [line]);
  const prevArea = useRef(area);
  useEffect(() => {
    if (prevArea.current !== area) {
      setCell("");
      setEdge("");
    }
    prevArea.current = area;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [area]);
  const prevCell = useRef(cell);
  useEffect(() => {
    if (prevCell.current !== cell) {
      setEdge("");
    }
    prevCell.current = cell;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [cell]);

  const hasSelection = Boolean(line || area || cell || edge);

  return (
    <div className="flex flex-wrap items-center gap-2">
      <Select value={site} onValueChange={setSite} disabled={allTags.isError}>
        <SelectTrigger>
          <SelectValue placeholder={t("live.site")} />
        </SelectTrigger>
        <SelectContent>
          {sites.map((s) => (
            <SelectItem key={s} value={s}>
              {s}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <Select value={line} onValueChange={setLine}>
        <SelectTrigger>
          <SelectValue placeholder={t("live.line")} />
        </SelectTrigger>
        <SelectContent>
          {(linesQuery.data ?? []).map((l) => (
            <SelectItem key={l.code} value={l.code}>
              {l.name}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <Select value={area} onValueChange={setArea}>
        <SelectTrigger>
          <SelectValue placeholder={t("live.area")} />
        </SelectTrigger>
        <SelectContent>
          {(areasQuery.data ?? []).map((a) => (
            <SelectItem key={a.code} value={a.code}>
              {a.name}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <Select value={cell} onValueChange={setCell}>
        <SelectTrigger>
          <SelectValue placeholder={t("live.cell")} />
        </SelectTrigger>
        <SelectContent>
          {(cellsQuery.data ?? []).map((c) => (
            <SelectItem key={c.code} value={c.code}>
              {c.name}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <Select value={edge} onValueChange={setEdge}>
        <SelectTrigger>
          <SelectValue placeholder={t("live.edge")} />
        </SelectTrigger>
        <SelectContent>
          {edgeOptions.map((e) => (
            <SelectItem key={e} value={e}>
              {e}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      {hasSelection && (
        <Button
          variant="outline"
          size="sm"
          onClick={() => {
            setLine("");
            setArea("");
            setCell("");
            setEdge("");
          }}
        >
          {t("live.clearFilters")}
        </Button>
      )}
      {allTags.isError && <span className="text-xs text-destructive">{t("live.siteError")}</span>}
    </div>
  );
}
