# web-ui-v2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `web-ui-v2`, a second, independent operator frontend (Vite + TanStack Router) that runs in parallel to the existing Next.js `web-ui` on a separate port, rebuilding only the two pages that are actually used (`Live`, `History`) on top of vendored `@ifahub/ui`/`@ifahub/tables` components, with zero risk to the current production frontend during development.

**Architecture:** New Vite SPA in `web-ui-v2/`, served in production by nginx (which also replaces Next.js's build-time-baked API proxy with a container-start-time `envsubst` template — the same bug class as the `CENTRAL_API_UPSTREAM` incident, structurally avoided). Own Docker image, own compose service on port 3002, own CI/CD workflow mirroring the existing `web-ui.yml` pattern. Component library and DataTable system are vendored (copied and adapted, not installed as a live cross-repo dependency) from `D:\ifaplatform\ifahub`.

**Tech Stack:** Vite, React 19, TanStack Router, TanStack Query v5, TanStack Table v8, Tailwind CSS v4, Radix UI (via `radix-ui`), `class-variance-authority`, `clsx`/`tailwind-merge`, i18next/react-i18next, Vitest + React Testing Library (new — see Task 0), Playwright (e2e, matching the manual-verification pattern already used this session).

**Spec:** `docs/superpowers/specs/2026-08-20-web-ui-v2-rewrite-design.md`

## Global Constraints

- React 19, Vite (not Next.js), TanStack Router/Query/Table, Tailwind CSS v4 — exact versions pinned to whatever `D:\ifaplatform\ifahub\libs\ui\package.json` and `apps\ifa-web\package.json` currently specify (peer deps: `react`/`react-dom` `^19`, `radix-ui` `^1`, `tailwindcss` `^4`, `@tanstack/react-query` `^5`, `@tanstack/react-table` `^8`).
- This is a second frontend. The existing `web-ui` (Next.js, port 3001) is never modified, and never stopped, by any task in this plan.
- Default/fallback language is Spanish (`es`), matching `apps/ifa-web/src/lib/i18n.ts`'s pattern exactly.
- No login/session implementation. Only structural readiness (a single header-injection point in the API client; route tree left able to grow a `beforeLoad` guard later).
- No `central-server` domain-model changes beyond what a root-caused bug fix in Tasks 9/11 actually requires.
- Every task that touches product code (not pure infra) gets its own PR for review, per this session's established pattern. Infra tasks may be committed directly to the feature branch this plan works from and reviewed together in that branch's own PR.
- Every page/component task must be verified against the real local stack (`docker-compose.scada.yml` + `docker-compose.edge-sim.yml`, project name `ifascada`) before being marked done — this stack is already running as of this plan being written.

---

### Task 0: Testing harness decision — add Vitest + React Testing Library

**Files:**
- Create: `web-ui-v2/vitest.config.ts`
- Create: `web-ui-v2/src/test/setup.ts`
- Modify: `web-ui-v2/package.json` (add `test`/`test:watch` scripts once the package exists — this task's own steps create the package first)

**Interfaces:**
- Produces: a `npm test` command any later task's "write the failing test" step can call. Test files live next to the code they test, named `*.test.ts`/`*.test.tsx`.

This project has zero component-test infrastructure today (verified: no Jest/Vitest/RTL config anywhere in the ifascada repo). Given the scale of this rewrite — vendored components, new filtering/selection logic, a root-caused connectivity bug fix — real automated tests catch regressions Playwright-driven manual verification won't (Playwright is kept too, for full end-to-end flows against the real stack; the two are complementary, not a choice between them).

- [ ] **Step 1: Scaffold the Vite project**

```bash
cd D:/ifascada/.claude/worktrees/cicd-central-webui-pipeline
npm create vite@latest web-ui-v2 -- --template react-ts
```

- [ ] **Step 2: Install the runtime and test dependencies, pinned to ifahub's versions**

```bash
cd web-ui-v2
npm install react@^19 react-dom@^19 @tanstack/react-router@^1 @tanstack/react-query@^5 @tanstack/react-table@^8 radix-ui@^1 class-variance-authority@^0.7 clsx@^2 tailwind-merge@^3 lucide-react@^1 i18next@^26 react-i18next@^17 cmdk@^1 sonner@^2 next-themes@^0.4
npm install -D tailwindcss@^4 @tailwindcss/vite@^4 vitest @vitest/ui jsdom @testing-library/react @testing-library/jest-dom @testing-library/user-event @tanstack/router-plugin @types/node
```

- [ ] **Step 3: Configure Vite (React plugin, TanStack Router plugin, Tailwind plugin, `@` path alias)**

```typescript
// web-ui-v2/vite.config.ts
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { tanstackRouter } from "@tanstack/router-plugin/vite";
import path from "node:path";

export default defineConfig({
  plugins: [tanstackRouter({ target: "react", autoCodeSplitting: true }), react(), tailwindcss()],
  resolve: {
    alias: { "@": path.resolve(__dirname, "./src") },
  },
  server: {
    port: 3002,
    proxy: {
      "/api": { target: "http://127.0.0.1:8088", changeOrigin: true },
      "/health": { target: "http://127.0.0.1:8088", changeOrigin: true },
    },
  },
});
```

The `server.proxy` block is for local `npm run dev` only — it's what makes `/api/*` reach the local `central-server` (port 8088) during development without CORS. Production uses nginx (Task 12), not this.

- [ ] **Step 4: Configure Vitest**

```typescript
// web-ui-v2/vitest.config.ts
import { defineConfig, mergeConfig } from "vitest/config";
import viteConfig from "./vite.config";

export default mergeConfig(
  viteConfig,
  defineConfig({
    test: {
      environment: "jsdom",
      setupFiles: ["./src/test/setup.ts"],
      globals: true,
    },
  })
);
```

```typescript
// web-ui-v2/src/test/setup.ts
import "@testing-library/jest-dom/vitest";
```

- [ ] **Step 5: Add test scripts to package.json**

```json
{
  "scripts": {
    "test": "vitest run",
    "test:watch": "vitest"
  }
}
```

- [ ] **Step 6: Write and run a throwaway smoke test to prove the harness works**

```typescript
// web-ui-v2/src/test/smoke.test.tsx
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";

describe("test harness smoke test", () => {
  it("renders a component and finds it by text", () => {
    render(<div>harness works</div>);
    expect(screen.getByText("harness works")).toBeInTheDocument();
  });
});
```

Run: `npm test`
Expected: 1 passed. Delete `src/test/smoke.test.tsx` after confirming (its only job was proving the harness works).

- [ ] **Step 7: Commit**

```bash
git add web-ui-v2/
git commit -m "chore(web-ui-v2): scaffold Vite + React 19 project with Vitest/RTL"
```

---

### Task 1: Tailwind CSS v4 base styles and fonts

**Files:**
- Create: `web-ui-v2/src/styles/globals.css`
- Modify: `web-ui-v2/src/main.tsx` (import the stylesheet)

**Interfaces:**
- Produces: `src/styles/globals.css`, imported once at the app root. All later vendored/new components rely on the CSS custom properties and `@theme`/dark-mode variant this file defines.

- [ ] **Step 1: Copy the base stylesheet from ifahub, dropping ifahub-specific brand assets**

```bash
cd D:/ifascada/.claude/worktrees/cicd-central-webui-pipeline
mkdir -p web-ui-v2/src/styles
cp D:/ifaplatform/ifahub/libs/ui/src/globals.css web-ui-v2/src/styles/globals.css
```

Open `web-ui-v2/src/styles/globals.css` and remove the `@font-face` blocks referencing `./fonts/sora-variable.woff2` and other ifahub-brand font files (those font files aren't being vendored — this is a visual detail, not a functional dependency). Replace the `font-family` values in the `@theme`/root variables with a system font stack (`ui-sans-serif, system-ui, sans-serif`) so nothing 404s.

- [ ] **Step 2: Import it once at the app root**

```typescript
// web-ui-v2/src/main.tsx (add near the top, before other imports that render UI)
import "./styles/globals.css";
```

- [ ] **Step 3: Verify Tailwind actually compiles by using a utility class**

```bash
cd web-ui-v2
npm run dev
```

Open `http://127.0.0.1:3002` (the Vite dev server), confirm no console errors about missing CSS/Tailwind, and confirm the page background is the dark theme color defined in `globals.css`'s root variables (not browser-default white) — Tailwind's `@import "tailwindcss"` and the custom theme variables are both being picked up.

- [ ] **Step 4: Commit**

```bash
git add web-ui-v2/src/styles/globals.css web-ui-v2/src/main.tsx
git commit -m "feat(web-ui-v2): add Tailwind v4 base styles vendored from ifahub"
```

---

### Task 2: Vendor `@ifahub/ui` primitives

**Files:**
- Create: `web-ui-v2/src/lib/utils.ts`
- Create: `web-ui-v2/src/components/ui/*.tsx` (one file per vendored primitive, listed below)
- Test: `web-ui-v2/src/components/ui/button.test.tsx` (representative smoke test; same pattern applies to any primitive worth a dedicated test later)

**Interfaces:**
- Produces: `cn()` from `@/lib/utils`, and each of `Button`, `Card`, `Table`/`TableHeader`/`TableBody`/`TableRow`/`TableHead`/`TableCell`, `Sidebar` (+ its sub-parts), `Select`, `Form` (+ react-hook-form bindings), `Sheet`, `Dialog`, `AlertDialog`, `Command`, `Badge`, `Tabs`, `Input`, `Label`, `Checkbox`, `DropdownMenu`, `Separator`, `Skeleton`, `Sonner` (toast), `Tooltip`, `ScrollArea`, `Popover`, `Breadcrumb`, `Collapsible`, `Switch`, `Textarea` from `@/components/ui/<name>` — the exact same component names and prop shapes as `libs/ui/src/ui/*.tsx` in ifahub, since these are copied files, not reimplementations.

This task does not need `react-hook-form`/`@hookform/resolvers`/`zod` installed yet — `form.tsx` is vendored now but nothing in `web-ui-v2` calls it until a later task actually needs a form (History's `Value > x` filter input doesn't need react-hook-form; it's simple local state). Install those three packages only when a task actually imports `form.tsx`.

- [ ] **Step 1: Vendor the `cn()` utility**

```bash
cd D:/ifascada/.claude/worktrees/cicd-central-webui-pipeline
mkdir -p web-ui-v2/src/lib web-ui-v2/src/components/ui
cp D:/ifaplatform/ifahub/libs/ui/src/lib/utils.ts web-ui-v2/src/lib/utils.ts
```

Open the copied file and delete the `sapDisplay()` function (SAP-specific, not needed here) — keep only `cn()`.

- [ ] **Step 2: Copy each primitive file verbatim**

```bash
cd D:/ifascada/.claude/worktrees/cicd-central-webui-pipeline
for f in button card table sidebar select form sheet dialog alert-dialog command badge tabs input label checkbox dropdown-menu separator skeleton sonner tooltip scroll-area popover breadcrumb collapsible switch textarea; do
  cp "D:/ifaplatform/ifahub/libs/ui/src/ui/${f}.tsx" "web-ui-v2/src/components/ui/${f}.tsx"
done
```

- [ ] **Step 2: Fix the import paths in every copied file**

Each copied file imports `cn` from ifahub's own path alias (likely `@/lib/utils` already, matching what was just set up in Task 1's `vite.config.ts` alias — verify this, since if ifahub uses a different alias convention, every copied file needs its import line updated):

```bash
cd D:/ifascada/.claude/worktrees/cicd-central-webui-pipeline/web-ui-v2
grep -l "from \"@/lib/utils\"" src/components/ui/*.tsx | wc -l
```

If this returns 0 (imports use a different path), run a search-and-replace across `src/components/ui/*.tsx` to point at `@/lib/utils` (the alias this project's `vite.config.ts` defines). If it returns a nonzero count matching the number of vendored files, no changes needed — the alias already matches.

- [ ] **Step 3: Install any missing peer packages the copied files reference**

```bash
cd web-ui-v2
npx tsc --noEmit 2>&1 | grep "Cannot find module" | sort -u
```

For each reported missing module (expect things like `@radix-ui/react-*` sub-packages if `radix-ui`'s unified package doesn't re-export what a given file imports directly — check each file's import line against what `radix-ui@^1` actually exports before assuming a separate install is needed), install it:

```bash
npm install <missing-package>
```

Re-run `npx tsc --noEmit` until it reports zero "Cannot find module" errors from `src/components/ui/`.

- [ ] **Step 4: Write a smoke test proving one vendored primitive renders correctly in this project**

```typescript
// web-ui-v2/src/components/ui/button.test.tsx
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { Button } from "./button";

describe("Button (vendored from @ifahub/ui)", () => {
  it("renders its children and responds to variant prop", () => {
    render(<Button variant="destructive">Eliminar</Button>);
    const btn = screen.getByRole("button", { name: "Eliminar" });
    expect(btn).toBeInTheDocument();
  });
});
```

- [ ] **Step 5: Run it to verify it passes**

Run: `npm test -- button.test.tsx`
Expected: PASS. If it fails on a missing Radix primitive or a `cn()` import error, that's Step 3's job not done yet — go back.

- [ ] **Step 6: Commit**

```bash
git add web-ui-v2/src/lib/utils.ts web-ui-v2/src/components/ui/
git commit -m "feat(web-ui-v2): vendor @ifahub/ui shadcn-style primitives"
```

---

### Task 3: Vendor `@ifahub/tables`'s DataTable system

**Files:**
- Create: `web-ui-v2/src/components/data-table/*.tsx` (copied from `libs/tables/src/components/*.tsx`)
- Create: `web-ui-v2/src/components/data-table/hooks/*.ts(x)` (copied from `libs/tables/src/hooks/*`)
- Create: `web-ui-v2/src/components/data-table/types.ts`, `web-ui-v2/src/components/data-table/utils/tableSearch.ts`
- Create: `web-ui-v2/src/lib/use-can.ts` (new — the no-op permission stub the spec calls for)
- Test: `web-ui-v2/src/components/data-table/data-table.test.tsx`

**Interfaces:**
- Consumes: `@/components/ui/table`, `@/components/ui/*` (Task 2's vendored primitives), `@tanstack/react-table`.
- Produces: `DataTable`, `DataTableRoot`, `DataTableContent`, `DataTableToolbar`, `DataTableSearch`, `DataTableColumnsDialog`, `DataTablePagination`, `DataTableSavedViews`, `DataTableEmpty`, `DataTableError`, `DataTableLoading` from `@/components/data-table`; `useDataTableInstance`, `useGridViews`, `useTableSearchState` from `@/components/data-table/hooks`; `useCan(permission: string): boolean` from `@/lib/use-can` (always returns `true` today — this is the auth-readiness stub the spec's "door left open" section describes).

- [ ] **Step 1: Vendor the component and hook files**

```bash
cd D:/ifascada/.claude/worktrees/cicd-central-webui-pipeline
mkdir -p web-ui-v2/src/components/data-table/hooks web-ui-v2/src/components/data-table/utils
cp D:/ifaplatform/ifahub/libs/tables/src/components/*.tsx web-ui-v2/src/components/data-table/
cp D:/ifaplatform/ifahub/libs/tables/src/hooks/*.ts* web-ui-v2/src/components/data-table/hooks/
cp D:/ifaplatform/ifahub/libs/tables/src/types.ts web-ui-v2/src/components/data-table/types.ts
cp D:/ifaplatform/ifahub/libs/tables/src/utils/tableSearch.ts web-ui-v2/src/components/data-table/utils/tableSearch.ts
cp D:/ifaplatform/ifahub/libs/tables/src/index.ts web-ui-v2/src/components/data-table/index.ts
```

- [ ] **Step 2: Create the no-op `useCan` stub**

`useTableLayouts.ts` (vendored in Step 1, inside `hooks/`) imports `useCan` from `@ifahub/auth`. Point it at a local stub instead:

```typescript
// web-ui-v2/src/lib/use-can.ts
/**
 * Auth-readiness stub (see docs/superpowers/specs/2026-08-20-web-ui-v2-rewrite-design.md,
 * "Auth: door left open, not implemented"). No login exists yet, so every permission check
 * passes. When real OIDC/Authentik auth is added later, this becomes the single place that
 * changes -- callers (like DataTableSavedViews' permission-gated actions) don't change.
 */
export function useCan(_permission: string): boolean {
  return true;
}
```

Edit `web-ui-v2/src/components/data-table/hooks/useTableLayouts.ts`'s import line from `import { useCan } from "@ifahub/auth";` to `import { useCan } from "@/lib/use-can";`.

- [ ] **Step 3: Fix remaining import paths and install missing peers**

```bash
cd web-ui-v2
npx tsc --noEmit 2>&1 | grep "Cannot find module\|Cannot find name" | sort -u
```

Fix any `@ifahub/ui` import paths to `@/components/ui` (the vendored location from Task 2), and any `@ifahub/types`/`@ifahub/api-client` imports — check whether the specific type/function being imported is actually used by the file, or is dead code for this project's purposes; if used, define a minimal local equivalent in `web-ui-v2/src/components/data-table/types.ts` rather than vendoring those two unrelated packages wholesale.

- [ ] **Step 4: Write a test that builds a real DataTable instance against a small in-memory dataset**

```typescript
// web-ui-v2/src/components/data-table/data-table.test.tsx
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { createColumnHelper } from "@tanstack/react-table";
import { DataTable } from "./DataTable";
import { useDataTableInstance } from "./hooks/useDataTableInstance";

type Row = { id: string; value: number };
const columnHelper = createColumnHelper<Row>();
const columns = [
  columnHelper.accessor("id", { header: "ID" }),
  columnHelper.accessor("value", { header: "Value" }),
];

function TestTable() {
  const table = useDataTableInstance({
    data: [{ id: "a", value: 1 }, { id: "b", value: 2 }],
    columns,
  });
  return <DataTable table={table} />;
}

describe("DataTable (vendored from @ifahub/tables)", () => {
  it("renders rows from the provided data", () => {
    render(<TestTable />);
    expect(screen.getByText("a")).toBeInTheDocument();
    expect(screen.getByText("b")).toBeInTheDocument();
  });
});
```

Adjust the exact prop names passed to `useDataTableInstance`/`DataTable` to match what the vendored files actually declare (read `web-ui-v2/src/components/data-table/hooks/useDataTableInstance.tsx` after vendoring — its real signature is the source of truth, not this sketch).

- [ ] **Step 5: Run it to verify it passes**

Run: `npm test -- data-table.test.tsx`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add web-ui-v2/src/components/data-table/ web-ui-v2/src/lib/use-can.ts
git commit -m "feat(web-ui-v2): vendor @ifahub/tables DataTable system with auth-readiness stub"
```

---

### Task 4: i18n bootstrap

**Files:**
- Create: `web-ui-v2/src/lib/i18n.ts`
- Create: `web-ui-v2/src/locales/es.ts`, `web-ui-v2/src/locales/en.ts`
- Modify: `web-ui-v2/src/main.tsx` (import `./lib/i18n` once)

**Interfaces:**
- Produces: a globally-initialized i18next instance. Any later component calls `useTranslation()` from `react-i18next` directly (standard hook, no wrapper needed — matches ifahub's own pattern of no explicit Provider).

- [ ] **Step 1: Vendor the `es` dictionaries from `@ifahub/ui` and `@ifahub/tables`**

```bash
cd D:/ifascada/.claude/worktrees/cicd-central-webui-pipeline
mkdir -p web-ui-v2/src/locales/vendor
cp D:/ifaplatform/ifahub/libs/ui/src/locales/es.ts web-ui-v2/src/locales/vendor/ui-es.ts
cp D:/ifaplatform/ifahub/libs/tables/src/locales/es.ts web-ui-v2/src/locales/vendor/tables-es.ts
```

- [ ] **Step 2: Write the ifascada-specific dictionary (Spanish; keys this plan's later page tasks will use)**

```typescript
// web-ui-v2/src/locales/es.ts
import { es as uiEs } from "./vendor/ui-es";
import { es as tablesEs } from "./vendor/tables-es";

export const es = {
  ...uiEs,
  ...tablesEs,
  nav: {
    live: "En vivo",
    history: "Histórico",
  },
  live: {
    title: "Estado en vivo",
    edgesOnline: "Edges en línea",
    site: "Sitio",
    line: "Línea",
    area: "Área",
    cell: "Celda",
    edge: "Edge",
  },
  history: {
    title: "Consulta histórica",
    tag: "Tag",
    pageSize: "Tamaño de página",
    valueFilter: "Valor >",
    unit: "Unidad",
    printSelected: "Imprimir seleccionados",
    selectedCount: "Seleccionados",
  },
};
```

- [ ] **Step 3: Write the English dictionary (structural placeholder for a future language, matching ifahub's pattern of shipping `en` alongside `es` from day one — not implementing a language switcher, just leaving the resource bundle shape ready)**

```typescript
// web-ui-v2/src/locales/en.ts
export const en = {
  nav: {
    live: "Live",
    history: "History",
  },
  live: {
    title: "Live status",
    edgesOnline: "Edges online",
    site: "Site",
    line: "Line",
    area: "Area",
    cell: "Cell",
    edge: "Edge",
  },
  history: {
    title: "Historical query",
    tag: "Tag",
    pageSize: "Page size",
    valueFilter: "Value >",
    unit: "Unit",
    printSelected: "Print selected",
    selectedCount: "Selected",
  },
};
```

- [ ] **Step 4: Bootstrap i18next, verbatim pattern from `apps/ifa-web/src/lib/i18n.ts`**

```typescript
// web-ui-v2/src/lib/i18n.ts
import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import { es } from "../locales/es";
import { en } from "../locales/en";

void i18n.use(initReactI18next).init({
  resources: { es: { translation: es }, en: { translation: en } },
  lng: "es",
  fallbackLng: "es",
  interpolation: { escapeValue: false },
  returnNull: false,
});

export default i18n;
```

- [ ] **Step 5: Import it once at the app root**

```typescript
// web-ui-v2/src/main.tsx (add alongside the globals.css import from Task 1)
import "./lib/i18n";
```

- [ ] **Step 6: Write a test proving a component using `useTranslation` renders the Spanish string**

```typescript
// web-ui-v2/src/locales/i18n.test.tsx
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { useTranslation } from "react-i18next";
import "../lib/i18n";

function Probe() {
  const { t } = useTranslation();
  return <span>{t("history.printSelected")}</span>;
}

describe("i18n bootstrap", () => {
  it("resolves a known key to its Spanish translation by default", () => {
    render(<Probe />);
    expect(screen.getByText("Imprimir seleccionados")).toBeInTheDocument();
  });
});
```

- [ ] **Step 7: Run it to verify it passes**

Run: `npm test -- i18n.test.tsx`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add web-ui-v2/src/lib/i18n.ts web-ui-v2/src/locales/
git commit -m "feat(web-ui-v2): bootstrap i18next with Spanish default, vendored ifahub dictionaries"
```

---

### Task 5: API client with future-auth injection point

**Files:**
- Create: `web-ui-v2/src/lib/api-client.ts`
- Create: `web-ui-v2/src/lib/api-client.test.ts`
- Create: `web-ui-v2/.env.development` (local dev proxy target, already handled by Task 0's `vite.config.ts` `server.proxy` — this file is for any `VITE_*` values components read directly, not the proxy itself)

**Interfaces:**
- Produces: `getJson<T>(path: string): Promise<T>`, `fetchTagsCurrent`, `fetchTagHistory`, `fetchEdgesCurrent`, `postEdgeAction` — same function names and shapes as the current `web-ui/lib/api.ts` (this is a straight port of that file's contract, not a redesign; only the transport underneath changes).

This is a near-direct port of `web-ui/lib/api.ts`'s existing functions (already proven correct against the real `central-server` API this whole session) into `web-ui-v2`, with one addition: a single point where an `Authorization` header would be injected once real auth exists.

- [ ] **Step 1: Write the failing test for the auth-injection point**

```typescript
// web-ui-v2/src/lib/api-client.test.ts
import { describe, it, expect, vi, beforeEach } from "vitest";
import { getJson, getAuthHeader } from "./api-client";

describe("getAuthHeader", () => {
  it("returns an empty object today (no auth implemented yet)", () => {
    expect(getAuthHeader()).toEqual({});
  });
});

describe("getJson", () => {
  beforeEach(() => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify({ ok: true }), { status: 200 })));
  });

  it("calls fetch with the auth header spread into request headers", async () => {
    await getJson("/api/tags/current");
    const [, init] = (fetch as ReturnType<typeof vi.fn>).mock.calls[0];
    expect(init.headers).toMatchObject(getAuthHeader());
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `npm test -- api-client.test.ts`
Expected: FAIL with "Cannot find module './api-client'" (it doesn't exist yet).

- [ ] **Step 3: Write the implementation, porting the existing functions from `web-ui/lib/api.ts`**

```typescript
// web-ui-v2/src/lib/api-client.ts
/**
 * Single point where an Authorization header would be added once real auth exists
 * (see the spec's "Auth: door left open" section). Empty today.
 */
export function getAuthHeader(): Record<string, string> {
  return {};
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(path, {
    ...init,
    headers: { ...getAuthHeader(), ...(init?.headers ?? {}) },
  });
  if (!res.ok) {
    throw new Error(`${init?.method ?? "GET"} ${path} failed: ${res.status}`);
  }
  return res.json() as Promise<T>;
}

export function getJson<T>(path: string): Promise<T> {
  return request<T>(path);
}

export type TagCurrent = {
  tag_code: string;
  device_code: string;
  site_code: string;
  line_code: string | null;
  area_code: string | null;
  cell_code: string | null;
  edge_code: string;
  ts: string;
  value: unknown;
  quality: { status?: string; reason?: string };
  source: string;
  metadata_json?: Record<string, unknown>;
  tag_status?: string;
  expected_interval_ms?: number | null;
};

export type EdgeCurrent = {
  site_code: string;
  line_code: string | null;
  area_code: string | null;
  cell_code: string | null;
  edge_code: string;
  status: string;
  last_seen_at: string;
  outbox_depth: number;
  outbox_oldest_secs: number | null;
  action_metrics: Record<string, unknown>;
};

export type TagHistory = {
  ts: string;
  site_code: string;
  edge_code: string;
  tag_code: string;
  value: unknown;
  quality_status: string;
};

type LiveFilter = { site?: string; line?: string; area?: string; cell?: string; edge?: string };

function toQuery(params: Record<string, string | number | undefined>): string {
  const qs = new URLSearchParams();
  Object.entries(params).forEach(([k, v]) => {
    if (v !== undefined && v !== "") qs.set(k, String(v));
  });
  return qs.toString();
}

export function fetchTagsCurrent(limit = 200, filter?: LiveFilter): Promise<TagCurrent[]> {
  const qs = toQuery({ limit, ...filter });
  return getJson<TagCurrent[]>(`/api/tags/current?${qs}`);
}

export function fetchEdgesCurrent(limit = 200, filter?: LiveFilter): Promise<EdgeCurrent[]> {
  const qs = toQuery({ limit, ...filter });
  return getJson<EdgeCurrent[]>(`/api/edges/current?${qs}`);
}

export function fetchTagHistory(tagCode: string, limit = 200, offset = 0): Promise<TagHistory[]> {
  return getJson<TagHistory[]>(`/api/tags/${encodeURIComponent(tagCode)}/history?limit=${limit}&offset=${offset}`);
}

export function postEdgeAction(
  site: string,
  edge: string,
  actionType: string,
  payload: Record<string, unknown>,
  meta: { source: string; target: string }
): Promise<unknown> {
  return request(`/api/edges/${encodeURIComponent(edge)}/actions`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ site, action_type: actionType, payload, ...meta }),
  });
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `npm test -- api-client.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add web-ui-v2/src/lib/api-client.ts web-ui-v2/src/lib/api-client.test.ts
git commit -m "feat(web-ui-v2): port API client from web-ui with a single future-auth header point"
```

---

### Task 6: Route tree and app shell (sidebar with only Live + History)

**Files:**
- Create: `web-ui-v2/src/routes/__root.tsx`, `web-ui-v2/src/routes/live.tsx`, `web-ui-v2/src/routes/history.tsx`, `web-ui-v2/src/routes/index.tsx`
- Create: `web-ui-v2/src/components/app-shell.tsx`
- Create: `web-ui-v2/src/main.tsx` (router setup; Tasks 1/4 already added imports here)
- Test: `web-ui-v2/src/components/app-shell.test.tsx`

**Interfaces:**
- Produces: a working router with two real routes (`/live`, `/history`) and a root redirect from `/` to `/live` (there is no Overview page in this app — confirmed removed per spec). `AppShell` renders the vendored `Sidebar` (Task 2) with exactly two nav links, using `t("nav.live")`/`t("nav.history")` (Task 4).

- [ ] **Step 1: Write the app shell using the vendored Sidebar**

```typescript
// web-ui-v2/src/components/app-shell.tsx
import { Link, Outlet } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import {
  Sidebar,
  SidebarContent,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuItem,
  SidebarMenuButton,
  SidebarProvider,
  SidebarInset,
} from "@/components/ui/sidebar";

export function AppShell() {
  const { t } = useTranslation();
  return (
    <SidebarProvider>
      <Sidebar>
        <SidebarHeader>IFASCADA</SidebarHeader>
        <SidebarContent>
          <SidebarMenu>
            <SidebarMenuItem>
              <SidebarMenuButton asChild>
                <Link to="/live">{t("nav.live")}</Link>
              </SidebarMenuButton>
            </SidebarMenuItem>
            <SidebarMenuItem>
              <SidebarMenuButton asChild>
                <Link to="/history">{t("nav.history")}</Link>
              </SidebarMenuButton>
            </SidebarMenuItem>
          </SidebarMenu>
        </SidebarContent>
      </Sidebar>
      <SidebarInset>
        <Outlet />
      </SidebarInset>
    </SidebarProvider>
  );
}
```

The exact prop names of `Sidebar`/`SidebarMenuButton`/etc. must match what Task 2 actually vendored — read `web-ui-v2/src/components/ui/sidebar.tsx` after vendoring and adjust this file's usage to match its real exported API if it differs from this sketch.

- [ ] **Step 2: Define the route tree**

```typescript
// web-ui-v2/src/routes/__root.tsx
import { createRootRoute } from "@tanstack/react-router";
import { AppShell } from "@/components/app-shell";

export const Route = createRootRoute({ component: AppShell });
```

```typescript
// web-ui-v2/src/routes/index.tsx
import { createFileRoute, redirect } from "@tanstack/react-router";

export const Route = createFileRoute("/")({
  beforeLoad: () => {
    throw redirect({ to: "/live" });
  },
});
```

```typescript
// web-ui-v2/src/routes/live.tsx
import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/live")({
  component: () => <div>Live placeholder (Task 9 replaces this)</div>,
});
```

```typescript
// web-ui-v2/src/routes/history.tsx
import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/history")({
  component: () => <div>History placeholder (Task 11 replaces this)</div>,
});
```

- [ ] **Step 3: Wire the router and providers in the app entry point**

```typescript
// web-ui-v2/src/main.tsx
import "./styles/globals.css";
import "./lib/i18n";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { RouterProvider, createRouter } from "@tanstack/react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { routeTree } from "./routeTree.gen";

const router = createRouter({ routeTree });
declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}

const queryClient = new QueryClient();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>
  </StrictMode>
);
```

(`routeTree.gen.ts` is auto-generated by the `@tanstack/router-plugin` configured in Task 0 — it appears after `npm run dev` runs once.)

- [ ] **Step 4: Write a test that the shell renders both nav links**

```typescript
// web-ui-v2/src/components/app-shell.test.tsx
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { RouterProvider, createRouter, createRootRoute, createRoute } from "@tanstack/react-router";
import { AppShell } from "./app-shell";
import "../lib/i18n";

describe("AppShell", () => {
  it("renders links to Live and History, and nothing else", () => {
    const rootRoute = createRootRoute({ component: AppShell });
    const liveRoute = createRoute({ getParentRoute: () => rootRoute, path: "/live", component: () => <div>live</div> });
    const routeTree = rootRoute.addChildren([liveRoute]);
    const router = createRouter({ routeTree, history: undefined });
    render(<RouterProvider router={router} />);
    expect(screen.getByText("En vivo")).toBeInTheDocument();
    expect(screen.getByText("Histórico")).toBeInTheDocument();
    expect(screen.queryByText(/overview|trends|commands|audit/i)).not.toBeInTheDocument();
  });
});
```

- [ ] **Step 5: Run npm run dev, confirm in a real browser via Playwright that `/` redirects to `/live` and the sidebar shows exactly two links**

```bash
cd web-ui-v2
npm run dev
```

Navigate to `http://127.0.0.1:3002/` with Playwright, take a snapshot, and confirm: URL redirected to `/live`, sidebar contains exactly "En vivo" and "Histórico" (no Overview/Trends/Commands/Audit entries anywhere).

- [ ] **Step 6: Commit**

```bash
git add web-ui-v2/src/routes/ web-ui-v2/src/components/app-shell.tsx web-ui-v2/src/components/app-shell.test.tsx web-ui-v2/src/main.tsx
git commit -m "feat(web-ui-v2): route tree and app shell with Live/History-only nav"
```

---

### Task 7: Site as a real dropdown (shared context bar)

**Files:**
- Create: `web-ui-v2/src/store/context-store.ts`
- Create: `web-ui-v2/src/components/context-bar.tsx`
- Test: `web-ui-v2/src/components/context-bar.test.tsx`

**Interfaces:**
- Consumes: `fetchTagsCurrent` (Task 5) to derive the distinct list of real `site_code` values currently reporting data (there's no dedicated "list of sites" endpoint today — confirmed by checking `crates/central-server/src/api.rs`'s route list before assuming otherwise; if no such endpoint exists, deriving the list from `fetchTagsCurrent`'s results is the correct fallback, not a placeholder).
- Produces: `useOperationalContextStore()` returning `{ site, line, area, cell, edge, setSite, setLine, setArea, setCell, setEdge }` — same shape as the current `web-ui/store/context-store.ts`, so ports of Live/History logic don't need to change their usage of it.

- [ ] **Step 1: Check whether `central-server` already exposes a sites/hierarchy listing endpoint**

```bash
cd D:/ifascada/.claude/worktrees/cicd-central-webui-pipeline
grep -n "\"/api/sites\|/sites\"\|context.*hierarchy\|hierarchy.*route" crates/central-server/src/api.rs
```

If a real endpoint exists, use it directly in Step 3 instead of deriving from `fetchTagsCurrent`. If not (expected, based on this session's earlier reading of `api.rs`), proceed with the derivation approach below.

- [ ] **Step 2: Write the store (unchanged shape from the current app, ported)**

```typescript
// web-ui-v2/src/store/context-store.ts
import { create } from "zustand";

type OperationalContextStore = {
  site: string;
  line: string;
  area: string;
  cell: string;
  edge: string;
  setSite: (v: string) => void;
  setLine: (v: string) => void;
  setArea: (v: string) => void;
  setCell: (v: string) => void;
  setEdge: (v: string) => void;
};

export const useOperationalContextStore = create<OperationalContextStore>((set) => ({
  site: "plant-a",
  line: "",
  area: "",
  cell: "",
  edge: "",
  setSite: (site) => set({ site }),
  setLine: (line) => set({ line }),
  setArea: (area) => set({ area }),
  setCell: (cell) => set({ cell }),
  setEdge: (edge) => set({ edge }),
}));
```

`zustand` needs installing: `npm install zustand@^5` (matching the version `apps/ifa-web` uses).

- [ ] **Step 3: Write the context bar, with Site as a real `<Select>` (vendored Task 2) instead of a text input**

```typescript
// web-ui-v2/src/components/context-bar.tsx
import { useQuery } from "@tanstack/react-query";
import { fetchTagsCurrent } from "@/lib/api-client";
import { useOperationalContextStore } from "@/store/context-store";
import { Select, SelectTrigger, SelectValue, SelectContent, SelectItem } from "@/components/ui/select";
import { useTranslation } from "react-i18next";

export function ContextBar() {
  const { t } = useTranslation();
  const { site, setSite } = useOperationalContextStore();
  // No dedicated "list of sites" endpoint exists today (verified against api.rs). Deriving
  // the real, currently-reporting site list from tag data instead of a hardcoded/free-text
  // field is what actually fixes the "Site is fixed text" complaint.
  const allTags = useQuery({ queryKey: ["all-sites-probe"], queryFn: () => fetchTagsCurrent(1000) });
  const sites = Array.from(new Set((allTags.data ?? []).map((t) => t.site_code))).sort();

  return (
    <Select value={site} onValueChange={setSite}>
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
  );
}
```

- [ ] **Step 4: Write a test with a mocked query result**

```typescript
// web-ui-v2/src/components/context-bar.test.tsx
import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ContextBar } from "./context-bar";
import * as apiClient from "@/lib/api-client";
import "../lib/i18n";

describe("ContextBar", () => {
  it("renders Site as a dropdown populated from real tag data, not free text", async () => {
    vi.spyOn(apiClient, "fetchTagsCurrent").mockResolvedValue([
      { site_code: "plant-a" } as never,
      { site_code: "plant-b" } as never,
    ]);
    const qc = new QueryClient();
    render(
      <QueryClientProvider client={qc}>
        <ContextBar />
      </QueryClientProvider>
    );
    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
    expect(await screen.findByRole("combobox")).toBeInTheDocument();
  });
});
```

- [ ] **Step 5: Run it to verify it passes**

Run: `npm test -- context-bar.test.tsx`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add web-ui-v2/src/store/context-store.ts web-ui-v2/src/components/context-bar.tsx web-ui-v2/src/components/context-bar.test.tsx
git commit -m "feat(web-ui-v2): Site as a real dropdown derived from live tag data"
```

---

### Task 8: Root-cause investigation — "edges online 0/n" always reads 0

**Files:**
- No files created by this task — it produces a written finding (added to this plan's Task 9, or to a new `docs/finding-*.md` if the root cause turns out to be a `central-server` bug worth documenting for other consumers, matching this repo's existing convention of `docs/finding-*.md` files).

Per `superpowers:systematic-debugging`, Phase 1: reproduce, read errors/logs carefully, check recent changes, gather evidence at each layer before touching any fix.

- [ ] **Step 1: Reproduce against the real local stack**

```bash
powershell -NoProfile -Command "(Invoke-WebRequest http://127.0.0.1:8088/api/edges/current -UseBasicParsing).Content"
```

Compare the count of edges with `status: "ok"` (or whatever the real status field/value is) in this response against whatever the frontend's "edges online 0/n" display is actually computing. Confirm: is the backend already returning the right online count and the frontend miscounts it, or does the backend's response itself not reflect edges we know are actually online (e.g., the running `edge-sim-*` containers)?

- [ ] **Step 2: If the backend response looks wrong, trace where `EdgeCurrent.status` is computed**

```bash
cd D:/ifascada/.claude/worktrees/cicd-central-webui-pipeline
grep -n "fn.*edges_current\|EdgeCurrent\|status.*online\|is_online\|staleness\|stale_after" crates/central-server/src/api.rs crates/central-server/src/persistence/postgres.rs crates/domain/src/device/status.rs
```

Read every match. The domain concept of "online"/"connected" is almost certainly time-window-based (a `last_seen_at` compared against some staleness threshold, similar to the `stale_after_secs`/`tag_status` pattern already seen in `TagCurrent`). Confirm whether `edges_current`'s query actually applies that same freshness logic, or returns a raw row regardless of staleness — a mismatch here would explain "always 0" if, for instance, the query's `WHERE` clause on a staleness column is inverted or comparing against the wrong timezone/epoch.

- [ ] **Step 3: If the backend is correct, trace the frontend counter**

If Step 1 shows the backend's `/api/edges/current` correctly reports online edges, the bug is in the *old* `web-ui`'s (Next.js) header component computing "edges online 0/n" — check `web-ui/components/context-bar.tsx` and `web-ui/components/app-shell.tsx` (the "edges online" string was seen in this session's earlier Playwright snapshots) for how it derives the numerator/denominator. This confirms whether `web-ui-v2`'s equivalent (Task 9, Live page) needs a different computation than what the old app does, or whether the old app's computation was simply never wired to real data at all.

- [ ] **Step 4: Write the finding down as a comment or dedicated doc, whichever the size of the root cause warrants**

If the root cause is a real backend bug (miscomputed staleness, wrong query), write `docs/finding-edges-online-counter-<short-description>.md` following this repo's existing `docs/finding-*.md` format (see `docs/finding-mqtt-client-stale-session-detection.md` for the expected structure: Status/Affects/Discovered/Summary/Evidence/Why this matters/Suggested fix). If it's simply a frontend wiring bug with no backend implication, a one-paragraph note in Task 9's PR description is enough — don't manufacture a finding doc for a trivial frontend miscount.

- [ ] **Step 5: Commit the finding doc if one was written**

```bash
git add docs/finding-*.md
git commit -m "docs(finding): root-cause the always-zero edges-online counter"
```

---

### Task 9: Live page

**Files:**
- Create: `web-ui-v2/src/routes/live.tsx` (replaces Task 6's placeholder)
- Create: `web-ui-v2/src/components/live/edges-online-badge.tsx`
- Test: `web-ui-v2/src/components/live/edges-online-badge.test.tsx`

**Interfaces:**
- Consumes: `fetchEdgesCurrent`, `fetchTagsCurrent` (Task 5), `useOperationalContextStore` (Task 7), whatever fix Task 8's investigation determined.
- Produces: the real Live page, plus `EdgesOnlineBadge` as its own tested unit (the fixed "edges online N/M" display, isolated from the rest of the page so its logic is independently testable regardless of what the rest of Live looks like).

- [ ] **Step 1: Write the failing test for the fixed counter, encoding Task 8's actual root cause**

```typescript
// web-ui-v2/src/components/live/edges-online-badge.test.tsx
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { EdgesOnlineBadge } from "./edges-online-badge";
import type { EdgeCurrent } from "@/lib/api-client";

const onlineEdge = { status: "ok", edge_code: "e1" } as EdgeCurrent;
const offlineEdge = { status: "disconnected", edge_code: "e2" } as EdgeCurrent;

describe("EdgesOnlineBadge", () => {
  it("counts only edges with an online status in the numerator", () => {
    render(<EdgesOnlineBadge edges={[onlineEdge, offlineEdge]} />);
    expect(screen.getByText("1/2")).toBeInTheDocument();
  });

  it("shows 0/0 with no edges rather than crashing", () => {
    render(<EdgesOnlineBadge edges={[]} />);
    expect(screen.getByText("0/0")).toBeInTheDocument();
  });
});
```

Adjust the `status` string literal(s) this test asserts on to match exactly what Task 8 found the real online/offline status values to be (this sketch uses `"ok"`/`"disconnected"` based on values already observed in this session's earlier `/api/edges/current` responses — confirm against Task 8's actual findings before relying on it).

- [ ] **Step 2: Run it to verify it fails**

Run: `npm test -- edges-online-badge.test.tsx`
Expected: FAIL (module doesn't exist).

- [ ] **Step 3: Implement it**

```typescript
// web-ui-v2/src/components/live/edges-online-badge.tsx
import { Badge } from "@/components/ui/badge";
import type { EdgeCurrent } from "@/lib/api-client";
import { useTranslation } from "react-i18next";

const ONLINE_STATUSES = new Set(["ok"]);

export function EdgesOnlineBadge({ edges }: { edges: EdgeCurrent[] }) {
  const { t } = useTranslation();
  const online = edges.filter((e) => ONLINE_STATUSES.has(e.status)).length;
  return (
    <Badge title={t("live.edgesOnline")}>
      {online}/{edges.length}
    </Badge>
  );
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `npm test -- edges-online-badge.test.tsx`
Expected: PASS.

- [ ] **Step 5: Build the Live route around it**

```typescript
// web-ui-v2/src/routes/live.tsx
import { createFileRoute } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { fetchEdgesCurrent, fetchTagsCurrent } from "@/lib/api-client";
import { useOperationalContextStore } from "@/store/context-store";
import { ContextBar } from "@/components/context-bar";
import { EdgesOnlineBadge } from "@/components/live/edges-online-badge";
import { useTranslation } from "react-i18next";

export const Route = createFileRoute("/live")({
  component: LivePage,
});

function LivePage() {
  const { t } = useTranslation();
  const { site, line, area, cell, edge } = useOperationalContextStore();
  const filter = { site, line: line || undefined, area: area || undefined, cell: cell || undefined, edge: edge || undefined };
  const edges = useQuery({
    queryKey: ["live-edges", filter],
    queryFn: () => fetchEdgesCurrent(200, filter),
    refetchInterval: 2500,
  });
  const tags = useQuery({
    queryKey: ["live-tags", filter],
    queryFn: () => fetchTagsCurrent(1000, filter),
    refetchInterval: 2500,
  });

  return (
    <div className="p-4 space-y-4">
      <div className="flex items-center gap-4">
        <ContextBar />
        <EdgesOnlineBadge edges={edges.data ?? []} />
      </div>
      <h1 className="text-lg font-semibold">{t("live.title")}</h1>
      {/* Tag/edge grid rendering ported from the current web-ui/app/live/page.tsx's JSX
          structure -- same data shape (TagCurrent[]/EdgeCurrent[]), just laid out with the
          vendored Card/Badge components instead of hand-rolled CSS classes. */}
    </div>
  );
}
```

- [ ] **Step 6: Verify against the real local stack with Playwright**

With `docker-compose.scada.yml` + `docker-compose.edge-sim.yml` running (already up as of this plan) and `npm run dev` serving `web-ui-v2` on 3002, navigate to `http://127.0.0.1:3002/live`, take a snapshot, and confirm the edges-online count matches the number of `edge-sim-*` containers actually running (`docker ps --filter name=ifascada-edge-sim`) — this is the real regression test for Task 8's fix, on top of the unit test in Step 1.

- [ ] **Step 7: Commit**

```bash
git add web-ui-v2/src/routes/live.tsx web-ui-v2/src/components/live/
git commit -m "feat(web-ui-v2): Live page with fixed edges-online counter"
```

---

### Task 10: Root-cause investigation — Live shows tags "connected" with no real telemetry

**Files:**
- No files created — produces a written finding, likely extending `docs/finding-mqtt-client-stale-session-detection.md` rather than a new doc, since this is suspected to be the same underlying mechanism observed from a different angle.

- [ ] **Step 1: Reproduce with a controlled disconnect**

Using the real local stack, stop one edge simulator's MQTT connection without a clean shutdown (matching the finding doc's own repro sketch — a `docker network disconnect` or firewall rule against one `edge-sim-*` container, not a graceful `docker stop`):

```bash
docker network disconnect ifascada_default ifascada-edge-sim-pack-1
```

- [ ] **Step 2: Watch whether `central-server`'s `tags_current`/`edges_current` responses keep reporting that edge/its tags as connected after the disconnect**

```bash
powershell -NoProfile -Command "while (\$true) { (Invoke-WebRequest http://127.0.0.1:8088/api/edges/current -UseBasicParsing).Content | Select-String 'edge-pack-1'; Start-Sleep -Seconds 5 }"
```

Compare against `docs/finding-mqtt-client-stale-session-detection.md`'s documented mechanism: a `mqtt_consumer.rs` client whose `event_loop.poll()` doesn't observe the broker-side session drop keeps the domain's "last seen"/"connected" state frozen at its last real update, rather than the state timing out. If the same shape of bug is reproduced here (state stays "connected" well past when it should be marked stale), this confirms the shared root cause and Task 11 should point at the SAME code path the existing finding doc names (`crates/central-server/src/mqtt_consumer.rs`), not a new investigation from scratch.

- [ ] **Step 3: Reconnect the simulator and record findings**

```bash
docker network connect ifascada_default ifascada-edge-sim-pack-1
```

Append a new "Evidence" entry to `docs/finding-mqtt-client-stale-session-detection.md` (don't create a duplicate finding doc) documenting this session's repro: timestamps, what stayed "connected" and for how long, whether it self-recovered or needed the same kind of manual restart the original finding describes.

- [ ] **Step 4: Commit the updated finding**

```bash
git add docs/finding-mqtt-client-stale-session-detection.md
git commit -m "docs(finding): reproduce the stale-session bug via Live's connected-with-no-telemetry symptom"
```

---

### Task 11: Fix Live's stale-connection display (scope depends on Task 10's finding)

**Files:**
- Modify: whichever file(s) Task 10 identified as the actual root cause — most likely `crates/central-server/src/mqtt_consumer.rs` (backend fix, matching the finding doc's own "Suggested fix directions") and/or `web-ui-v2/src/routes/live.tsx`/`edges-online-badge.tsx` (a frontend staleness-threshold display fix, if the backend fix is out of scope for this plan and a `last_seen_at`-based client-side staleness cutoff is the pragmatic interim mitigation).

This task's exact steps depend on Task 10's finding, which isn't known yet — that's the point of doing the investigation first. Two concrete, real options this task chooses between (not placeholders — these are the two directions the existing finding doc's own "Suggested fix directions" section already lays out):

**Option A — the root cause is in `mqtt_consumer.rs`'s reconnect/staleness detection (matches the finding doc's suggestion #2: track time-since-last-broker-activity and force a reconnect past `keep_alive * 1.5`):**

- [ ] **Step 1: Write a failing Rust test reproducing the staleness-not-detected behavior**, in `crates/central-server/src/mqtt_consumer.rs`'s existing `#[cfg(test)] mod tests` block, using the same test patterns already in that file (check `crates/central-server/src/mqtt_consumer.rs` for its existing test setup before writing a new one from scratch).
- [ ] **Step 2: Run it, confirm it fails.**
- [ ] **Step 3: Implement the fix** (add the time-since-last-broker-activity tracking the finding doc's suggestion #2 describes; exact implementation depends on `rumqttc`'s API surface in the version this crate pins — read `crates/central-server/Cargo.toml`'s `rumqttc` version before writing this).
- [ ] **Step 4: Run it, confirm it passes,** plus run the full existing `mqtt_consumer` test suite (`cargo test -p central-server mqtt_consumer`) to confirm no regression.
- [ ] **Step 5: Verify against the real local stack** using the same disconnect repro as Task 10, confirming the edge now flips to "disconnected" within the expected bound instead of staying stuck.
- [ ] **Step 6: Commit.**

**Option B — the root cause is environmental/out of this plan's reach (e.g. needs infrastructure changes to the runner/broker this session already found fragile), and the pragmatic fix is a client-side staleness cutoff:**

- [ ] **Step 1: Write a failing test for a `isStale(lastSeenAt: string, thresholdMs: number): boolean` pure function** in `web-ui-v2/src/lib/staleness.test.ts`.
- [ ] **Step 2: Run it, confirm it fails.**
- [ ] **Step 3: Implement `isStale` in `web-ui-v2/src/lib/staleness.ts`**, and use it in `EdgesOnlineBadge`/the Live page's per-tag connection indicator instead of trusting the backend's `status` field verbatim — this bounds the damage in the UI even if the backend's state genuinely lags.
- [ ] **Step 4: Run it, confirm it passes.**
- [ ] **Step 5: Verify against the real local stack** using the same disconnect repro, confirming the UI now shows the edge as stale/disconnected within the threshold even if the backend's own field is still stuck on "ok".
- [ ] **Step 6: Commit.**

Pick the option based on what Task 10 actually found — implementing both is redundant; Option A fixes the real problem, Option B is the fallback if Option A turns out to be a bigger lift than this plan should absorb (in which case, document that decision in the PR description, since it means the finding doc's underlying bug is still open for future work).

---

### Task 12: History page

**Files:**
- Create: `web-ui-v2/src/routes/history.tsx` (replaces Task 6's placeholder)
- Create: `web-ui-v2/src/components/history/history-columns.tsx`
- Create: `web-ui-v2/src/components/history/print-selected-button.tsx`
- Create: `web-ui-v2/src/lib/value-formatting.ts` (unit-aware value display, port of `web-ui/lib/hmi-value.ts`'s `formatProcessValue`/`parseCompound` plus a new unit-extraction helper)
- Test: `web-ui-v2/src/lib/value-formatting.test.ts`
- Test: `web-ui-v2/src/components/history/history-columns.test.tsx`

**Interfaces:**
- Consumes: `fetchTagHistory`, `fetchTagsCurrent`, `postEdgeAction` (Task 5), `DataTable`/`useDataTableInstance` (Task 3).
- Produces: the rebuilt History page satisfying every item in the spec's History section: unit shown next to value, `Value > x` filter (default `x = 0`), `tag_code`/`site_code`/`edge_code` columns dropped, shift-click range multi-select for printing.

- [ ] **Step 1: Write the failing test for unit-aware value formatting**

```typescript
// web-ui-v2/src/lib/value-formatting.test.ts
import { describe, it, expect } from "vitest";
import { parseValueWithUnit, numericValue } from "./value-formatting";

describe("parseValueWithUnit", () => {
  it("splits a compound value string into number and unit", () => {
    expect(parseValueWithUnit("330 g")).toEqual({ number: 330, unit: "g" });
  });

  it("handles a plain number with no unit", () => {
    expect(parseValueWithUnit(42)).toEqual({ number: 42, unit: null });
  });

  it("handles a negative decimal with a unit", () => {
    expect(parseValueWithUnit("-8.05238 g")).toEqual({ number: -8.05238, unit: "g" });
  });
});

describe("numericValue", () => {
  it("extracts just the number for filtering purposes", () => {
    expect(numericValue("100 mg")).toBe(100);
    expect(numericValue(-5)).toBe(-5);
    expect(numericValue("not a number")).toBeNull();
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `npm test -- value-formatting.test.ts`
Expected: FAIL (module doesn't exist).

- [ ] **Step 3: Implement it**

```typescript
// web-ui-v2/src/lib/value-formatting.ts
export type ParsedValue = { number: number; unit: string | null };

export function parseValueWithUnit(value: unknown): ParsedValue | null {
  if (typeof value === "number") return { number: value, unit: null };
  if (typeof value === "string") {
    const match = value.trim().match(/^(-?\d+(?:\.\d+)?)\s*([a-zA-Z%]+)?$/);
    if (!match) return null;
    return { number: Number.parseFloat(match[1]), unit: match[2] ?? null };
  }
  return null;
}

export function numericValue(value: unknown): number | null {
  const parsed = parseValueWithUnit(value);
  return parsed ? parsed.number : null;
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `npm test -- value-formatting.test.ts`
Expected: PASS.

- [ ] **Step 5: Write the failing test for the History table's column definitions (proving tag/site/edge are gone, unit is shown)**

```typescript
// web-ui-v2/src/components/history/history-columns.test.tsx
import { describe, it, expect } from "vitest";
import { historyColumns } from "./history-columns";

describe("historyColumns", () => {
  it("does not include tag_code, site_code, or edge_code columns", () => {
    const ids = historyColumns.map((c) => c.id ?? (c as { accessorKey?: string }).accessorKey);
    expect(ids).not.toContain("tag_code");
    expect(ids).not.toContain("site_code");
    expect(ids).not.toContain("edge_code");
  });

  it("includes a unit column separate from the raw value column", () => {
    const ids = historyColumns.map((c) => c.id ?? (c as { accessorKey?: string }).accessorKey);
    expect(ids).toContain("unit");
  });
});
```

- [ ] **Step 6: Run it to verify it fails**

Run: `npm test -- history-columns.test.tsx`
Expected: FAIL (module doesn't exist).

- [ ] **Step 7: Implement the column definitions**

```typescript
// web-ui-v2/src/components/history/history-columns.tsx
import { createColumnHelper } from "@tanstack/react-table";
import type { TagHistory } from "@/lib/api-client";
import { parseValueWithUnit } from "@/lib/value-formatting";

const columnHelper = createColumnHelper<TagHistory>();

export const historyColumns = [
  columnHelper.accessor("ts", {
    id: "ts",
    header: "Timestamp",
    cell: (info) => new Date(info.getValue()).toLocaleString(),
  }),
  columnHelper.accessor((row) => parseValueWithUnit(row.value)?.number ?? null, {
    id: "value",
    header: "Value",
  }),
  columnHelper.accessor((row) => parseValueWithUnit(row.value)?.unit ?? "-", {
    id: "unit",
    header: "Unit",
  }),
  columnHelper.accessor("quality_status", {
    id: "quality_status",
    header: "Quality",
  }),
];
```

- [ ] **Step 8: Run the tests to verify they pass**

Run: `npm test -- history-columns.test.tsx`
Expected: PASS.

- [ ] **Step 9: Build the History route: DataTable, `Value > x` filter, shift-click range select, print button**

```typescript
// web-ui-v2/src/routes/history.tsx
import { createFileRoute } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import { fetchTagHistory, fetchTagsCurrent } from "@/lib/api-client";
import { useOperationalContextStore } from "@/store/context-store";
import { numericValue } from "@/lib/value-formatting";
import { historyColumns } from "@/components/history/history-columns";
import { DataTable } from "@/components/data-table/DataTable";
import { useDataTableInstance } from "@/components/data-table/hooks/useDataTableInstance";
import { Input } from "@/components/ui/input";
import { PrintSelectedButton } from "@/components/history/print-selected-button";
import { useTranslation } from "react-i18next";

const HISTORY_FETCH_LIMIT = 2000;

export const Route = createFileRoute("/history")({
  component: HistoryPage,
});

function HistoryPage() {
  const { t } = useTranslation();
  const { site } = useOperationalContextStore();
  const [selectedTag, setSelectedTag] = useState("");
  const [minValue, setMinValue] = useState(0);
  const [lastClickedIndex, setLastClickedIndex] = useState<number | null>(null);
  const [selectedRowIndexes, setSelectedRowIndexes] = useState<Set<number>>(new Set());

  const tags = useQuery({ queryKey: ["history-tags", site], queryFn: () => fetchTagsCurrent(500, { site }) });
  const history = useQuery({
    queryKey: ["history-events", selectedTag, HISTORY_FETCH_LIMIT],
    queryFn: () => fetchTagHistory(selectedTag, HISTORY_FETCH_LIMIT, 0),
    enabled: Boolean(selectedTag),
  });

  const filteredRows = useMemo(
    () => (history.data ?? []).filter((r) => {
      const n = numericValue(r.value);
      return n !== null && n > minValue;
    }),
    [history.data, minValue]
  );

  const table = useDataTableInstance({ data: filteredRows, columns: historyColumns });

  function handleRowClick(index: number, shiftKey: boolean) {
    setSelectedRowIndexes((prev) => {
      const next = new Set(prev);
      if (shiftKey && lastClickedIndex !== null) {
        const [start, end] = [lastClickedIndex, index].sort((a, b) => a - b);
        for (let i = start; i <= end; i++) next.add(i);
      } else {
        next.has(index) ? next.delete(index) : next.add(index);
      }
      return next;
    });
    setLastClickedIndex(index);
  }

  const selectedRows = Array.from(selectedRowIndexes)
    .map((i) => filteredRows[i])
    .filter((r): r is (typeof filteredRows)[number] => r !== undefined);

  return (
    <div className="p-4 space-y-4">
      <h1 className="text-lg font-semibold">{t("history.title")}</h1>
      <div className="flex items-center gap-4">
        <select value={selectedTag} onChange={(e) => setSelectedTag(e.target.value)}>
          <option value="">{t("history.tag")}</option>
          {(tags.data ?? []).map((tg) => (
            <option key={tg.tag_code} value={tg.tag_code}>
              {tg.tag_code} ({tg.device_code})
            </option>
          ))}
        </select>
        <label>
          {t("history.valueFilter")}
          <Input
            type="number"
            value={minValue}
            onChange={(e) => setMinValue(Number.parseFloat(e.target.value) || 0)}
          />
        </label>
        <span>
          {t("history.selectedCount")}: {selectedRows.length}
        </span>
        <PrintSelectedButton selectedRows={selectedRows} tagCode={selectedTag} />
      </div>
      <DataTable table={table} onRowClick={handleRowClick} selectedRowIndexes={selectedRowIndexes} />
    </div>
  );
}
```

The exact prop names `DataTable` accepts for row click/selection (`onRowClick`, `selectedRowIndexes`) must match Task 3's actually-vendored component — if `DataTableContent`/`DataTable` doesn't natively support a row-click callback, add one as a small wrapper around the vendored component rather than modifying the vendored file directly (keeps future re-vendoring from ifahub conflict-free).

- [ ] **Step 10: Write `PrintSelectedButton`, porting the print flow from `web-ui/app/history/page.tsx`'s `executePrintSelected`**

```typescript
// web-ui-v2/src/components/history/print-selected-button.tsx
import { useState } from "react";
import { postEdgeAction } from "@/lib/api-client";
import type { TagHistory } from "@/lib/api-client";
import { Button } from "@/components/ui/button";
import { useTranslation } from "react-i18next";

export function PrintSelectedButton({
  selectedRows,
  tagCode,
}: {
  selectedRows: TagHistory[];
  tagCode: string;
}) {
  const { t } = useTranslation();
  const [printing, setPrinting] = useState(false);

  async function handlePrint() {
    if (selectedRows.length === 0 || !tagCode) return;
    setPrinting(true);
    try {
      const bufferId = `ui:${tagCode}:${Date.now()}`;
      for (const row of [...selectedRows].sort((a, b) => new Date(a.ts).getTime() - new Date(b.ts).getTime())) {
        await postEdgeAction(
          row.site_code,
          row.edge_code,
          "buffer.weights.accumulate",
          { buffer_id: bufferId, trigger: { tag_id: tagCode, value: row.value, timestamp: row.ts } },
          { source: "web-ui-v2", target: "edge" }
        );
      }
      await postEdgeAction(
        selectedRows[0].site_code,
        selectedRows[0].edge_code,
        "device.command",
        { command: "print", args: { mode: "from_buffer", buffer_id: bufferId, clear_after_print: true } },
        { source: "web-ui-v2", target: "edge" }
      );
    } finally {
      setPrinting(false);
    }
  }

  return (
    <Button disabled={printing || selectedRows.length === 0} onClick={handlePrint}>
      {printing ? "..." : t("history.printSelected")}
    </Button>
  );
}
```

This is a simplified port — the current `web-ui/app/history/page.tsx` additionally reads a `print.persist` automation and a `device.command` payload template out of the selected tag's `metadata_json` (see that file's `findPrintDeviceCommand`/`findPrintPersistAction`). Port that same metadata-driven logic here before this task is done — the sketch above omits it only for brevity in this plan; the actual implementation must not silently drop that behavior, since it's what makes printing use the tag's real configured automation instead of a hardcoded command.

- [ ] **Step 11: Verify against the real local stack with Playwright**

Repeat the exact reproduction sequence already used earlier this session for the Next.js History page's cross-page-selection fix (select rows, use shift-click across a page boundary if the DataTable paginates, confirm the selected count accumulates correctly and print sends one `device.command` covering the full range) — now against `http://127.0.0.1:3002/history`.

- [ ] **Step 12: Commit**

```bash
git add web-ui-v2/src/routes/history.tsx web-ui-v2/src/components/history/ web-ui-v2/src/lib/value-formatting.ts web-ui-v2/src/lib/value-formatting.test.ts
git commit -m "feat(web-ui-v2): History page with unit display, Value>x filter, shift-click range select"
```

---

### Task 13: Investigate what central-server/edge-agent expose for edge event history and reset

**Files:**
- No files created — produces findings that Task 14 depends on.

**Already found while writing this plan** (saves re-discovering it): a real reset endpoint
exists today — `POST /api/edges/reset` → `edge_reset()` in `crates/central-server/src/api.rs`
(around line 956). Request body:

```json
{ "site_code": "plant-a", "edge_code": "edge-pack-1", "reason": "manual reset from diagnostics panel", "operator": null, "request_id": null }
```

Response: `{ "accepted": bool, "topic": string, "request_id": string | null }`. It publishes an
`EdgeResetCommandMessage` (schema_version 1) to MQTT topic
`scada/{site_code}/edge/{edge_code}/control/reset` with QoS 1 and returns `accepted: true` as
soon as the *publish to the broker* succeeds — **this is not proof the edge itself received or
acted on the reset**, only that central-server successfully handed the command to MQTT. This
is exactly the user's stated uncertainty ("no se si funciona") and must not be glossed over:
Step 1 below is about confirming whether an edge *actually resets* when this is called, not just
whether the HTTP call itself returns 200.

- [ ] **Step 1: Confirm end-to-end that a real edge simulator actually resets when this is called**

```bash
docker logs ifascada-edge-sim-pack-1 --since 1m
powershell -NoProfile -Command "Invoke-WebRequest -Method POST http://127.0.0.1:8088/api/edges/reset -Body (@{site_code='plant-a'; edge_code='edge-pack-1'; reason='diagnostics panel investigation'} | ConvertTo-Json) -ContentType 'application/json'"
docker logs ifascada-edge-sim-pack-1 --since 1m
```

Compare the two `docker logs` snapshots: does `edge-agent`'s log show it received and handled a
`control/reset` message (check `crates/edge-agent/src/main.rs`/`mqtt_bridge.rs` for what it logs
on that topic), and does it visibly do something (reconnect, reload config, restart its runtime
loop)? If nothing changes in the edge's logs despite the API returning `accepted: true`, that
confirms the exact gap the user suspected — the API "succeeding" doesn't mean the edge did
anything, and Task 14's "real feedback" must reflect that (see Task 14 Step 3).

- [ ] **Step 2: Check what `central-server`'s API already exposes for edge event history**

```bash
cd D:/ifascada/.claude/worktrees/cicd-central-webui-pipeline
grep -n "\"/api/.*event\|ops_events\|/api/edges/.*history\|OpsEvent" crates/central-server/src/api.rs
```

Cross-reference against `web-ui/lib/sse.ts`'s already-defined `OpsEvent` type (seen earlier this session) and `subscribeOpsSse` — this strongly suggests an ops-events stream already exists server-side; confirm whether there's also a *non-streaming* query endpoint (a "give me the last N events for this edge" REST call) suitable for a diagnostics panel that opens on demand, rather than requiring an always-open SSE connection. If only the SSE stream exists and no query endpoint does, this task's scope expands to add a minimal `GET /api/edges/{edge_code}/events?limit=N` route to `central-server`, following the same contract-test pattern as `crates/central-server/tests/api_connections_contract_tests.rs`.

- [ ] **Step 3: Document findings inline in this plan's Task 14** (update Task 14's own implementation once the event-history question from Step 2 is resolved — that part of this task's output is information the next task consumes, not code this task writes itself).

---

### Task 14: Edge diagnostics panel

**Files:**
- Create: `web-ui-v2/src/components/live/edge-diagnostics-panel.tsx` (using the vendored `Sheet` from Task 2 as the slide-out container)
- Test: `web-ui-v2/src/components/live/edge-diagnostics-panel.test.tsx`
- Modify: `web-ui-v2/src/routes/live.tsx` (wire up opening the panel from a disconnected/stale edge)

**Interfaces:**
- Consumes: `POST /api/edges/reset` (confirmed real, see Task 13 — body `{ site_code, edge_code, reason }`, response `{ accepted, topic, request_id }`), and whatever event-history endpoint Task 13 Step 2 found or added.
- Produces: `EdgeDiagnosticsPanel`, opened by clicking a disconnected edge on the Live page, showing recent events and a reset button whose feedback reflects Task 13's actual finding about whether `accepted: true` means the edge really reset — not just that the HTTP call succeeded.

- [ ] **Step 1: Write the failing test for the reset button's feedback states**

```typescript
// web-ui-v2/src/components/live/edge-diagnostics-panel.test.tsx
import { describe, it, expect, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { EdgeDiagnosticsPanel } from "./edge-diagnostics-panel";
import * as edgeActions from "@/lib/edge-actions";
import * as apiClient from "@/lib/api-client";

describe("EdgeDiagnosticsPanel reset action", () => {
  it("shows confirmed-recovered feedback once last_seen_at actually advances after reset", async () => {
    vi.spyOn(edgeActions, "resetEdge").mockResolvedValue({ accepted: true, topic: "x", request_id: null });
    vi.spyOn(apiClient, "fetchEdgesCurrent")
      .mockResolvedValueOnce([{ last_seen_at: "2026-08-20T10:00:00Z" } as never]) // before
      .mockResolvedValueOnce([{ last_seen_at: "2026-08-20T10:00:05Z" } as never]); // after, advanced
    render(<EdgeDiagnosticsPanel edgeCode="edge-pack-1" site="plant-a" open onOpenChange={() => {}} />);
    await userEvent.click(screen.getByRole("button", { name: /reset/i }));
    await waitFor(() => expect(screen.getByText(/reset confirmado/i)).toBeInTheDocument());
  });

  it("shows a no-recovery-confirmed warning when accepted:true but last_seen_at never advances", async () => {
    vi.spyOn(edgeActions, "resetEdge").mockResolvedValue({ accepted: true, topic: "x", request_id: null });
    vi.spyOn(apiClient, "fetchEdgesCurrent").mockResolvedValue([{ last_seen_at: "2026-08-20T10:00:00Z" } as never]);
    render(<EdgeDiagnosticsPanel edgeCode="edge-pack-1" site="plant-a" open onOpenChange={() => {}} />);
    await userEvent.click(screen.getByRole("button", { name: /reset/i }));
    await waitFor(
      () => expect(screen.getByText(/no confirm[oó] recuperaci[oó]n/i)).toBeInTheDocument(),
      { timeout: 35000 }
    );
  });

  it("shows an error state when the reset call itself fails (not just an unconfirmed recovery)", async () => {
    vi.spyOn(edgeActions, "resetEdge").mockRejectedValue(new Error("network error"));
    vi.spyOn(apiClient, "fetchEdgesCurrent").mockResolvedValue([{ last_seen_at: "2026-08-20T10:00:00Z" } as never]);
    render(<EdgeDiagnosticsPanel edgeCode="edge-pack-1" site="plant-a" open onOpenChange={() => {}} />);
    await userEvent.click(screen.getByRole("button", { name: /reset/i }));
    await waitFor(() => expect(screen.getByText(/error al enviar/i)).toBeInTheDocument());
  });
});
```

The second test's 35-second timeout matches the implementation's 15 attempts × 2000ms poll
interval (Step 3) plus margin — if that polling schedule changes, update this test's timeout to
match, not the other way around.

- [ ] **Step 2: Run it to verify it fails**

Run: `npm test -- edge-diagnostics-panel.test.tsx`
Expected: FAIL (module doesn't exist).

- [ ] **Step 3: Implement it against the real, confirmed `POST /api/edges/reset` endpoint**

The reset call's own `accepted: true` only proves central-server successfully published to MQTT
— per Task 13 Step 1's finding, that is not the same as the edge having reset. So "real feedback"
here means: show that the command was sent immediately, then separately poll whether the edge's
`last_seen_at` actually advances afterward (a real signal the edge came back up), rather than
treating the initial `200 accepted: true` response alone as "it worked".

```typescript
// web-ui-v2/src/lib/edge-actions.ts
export type ResetEdgeRequest = { site_code: string; edge_code: string; reason?: string };
export type ResetEdgeResponse = { accepted: boolean; topic: string; request_id: string | null };

export async function resetEdge(req: ResetEdgeRequest): Promise<ResetEdgeResponse> {
  const res = await fetch("/api/edges/reset", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(req),
  });
  if (!res.ok) throw new Error(`reset failed: ${res.status}`);
  return res.json();
}
```

```typescript
// web-ui-v2/src/components/live/edge-diagnostics-panel.tsx
import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { resetEdge } from "@/lib/edge-actions";
import { fetchEdgesCurrent } from "@/lib/api-client";
import { Sheet, SheetContent, SheetHeader, SheetTitle } from "@/components/ui/sheet";
import { Button } from "@/components/ui/button";

type ResetState = "idle" | "sent" | "confirmed-recovered" | "error" | "timed-out-no-recovery";

export function EdgeDiagnosticsPanel({
  edgeCode,
  site,
  open,
  onOpenChange,
}: {
  edgeCode: string;
  site: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const [resetState, setResetState] = useState<ResetState>("idle");

  // Task 13 Step 2's finding determines this query's real endpoint (an existing OpsEvent
  // history query, or one added as part of Task 13/this task). Left as a named query so the
  // panel's structure doesn't change once that's wired up.
  const events = useQuery({
    queryKey: ["edge-diagnostics-events", edgeCode],
    queryFn: () => fetchEdgeEvents(edgeCode),
    enabled: open,
  });

  async function handleReset() {
    setResetState("sent");
    const before = await fetchEdgesCurrent(1, { edge: edgeCode });
    const lastSeenBefore = before[0]?.last_seen_at;
    try {
      await resetEdge({ site_code: site, edge_code: edgeCode, reason: "manual reset from diagnostics panel" });
    } catch {
      setResetState("error");
      return;
    }
    // Poll for real recovery evidence -- the initial accepted:true only means the MQTT publish
    // succeeded, not that the edge came back. Give it a bounded window matching this project's
    // established health-poll pattern (scripts/lib/DeployDockerService.ps1's Test-ServiceHealthy).
    for (let attempt = 0; attempt < 15; attempt++) {
      await new Promise((r) => setTimeout(r, 2000));
      const after = await fetchEdgesCurrent(1, { edge: edgeCode });
      if (after[0]?.last_seen_at && after[0].last_seen_at !== lastSeenBefore) {
        setResetState("confirmed-recovered");
        return;
      }
    }
    setResetState("timed-out-no-recovery");
  }

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent>
        <SheetHeader>
          <SheetTitle>{edgeCode}</SheetTitle>
        </SheetHeader>
        <Button onClick={handleReset} disabled={resetState === "sent"}>
          Reset
        </Button>
        {resetState === "sent" && <p>Comando enviado, esperando confirmación del edge...</p>}
        {resetState === "confirmed-recovered" && <p>Reset confirmado: el edge volvió a reportar.</p>}
        {resetState === "timed-out-no-recovery" && (
          <p>El comando se envió, pero el edge no confirmó recuperación en 30s. Puede requerir intervención manual.</p>
        )}
        {resetState === "error" && <p>Error al enviar el comando de reset.</p>}
        <ul>
          {(events.data ?? []).map((e, i) => (
            <li key={i}>{JSON.stringify(e)}</li>
          ))}
        </ul>
      </SheetContent>
    </Sheet>
  );
}
```

`fetchEdgeEvents` is defined once Task 13 Step 2's investigation confirms the real endpoint shape — add it to `web-ui-v2/src/lib/api-client.ts` (Task 5) following that same file's existing function patterns.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `npm test -- edge-diagnostics-panel.test.tsx`
Expected: PASS.

- [ ] **Step 5: Wire it into the Live page**

Modify `web-ui-v2/src/routes/live.tsx` (Task 9) so clicking a disconnected/stale edge's row opens `EdgeDiagnosticsPanel` with that edge's code and site.

- [ ] **Step 6: Verify against the real local stack**

Disconnect an edge simulator the same way as Task 10's repro, open its diagnostics panel in the running app, click Reset, and confirm real success/failure feedback appears (not silence) — reconnect the simulator container afterward.

- [ ] **Step 7: Commit**

```bash
git add web-ui-v2/src/components/live/edge-diagnostics-panel.tsx web-ui-v2/src/components/live/edge-diagnostics-panel.test.tsx web-ui-v2/src/routes/live.tsx
git commit -m "feat(web-ui-v2): edge diagnostics panel with working reset and real feedback"
```

---

### Task 15: Production Dockerfile with nginx reverse proxy

**Files:**
- Create: `web-ui-v2/Dockerfile`
- Create: `web-ui-v2/nginx.conf.template`
- Create: `web-ui-v2/docker-entrypoint.sh`

**Interfaces:**
- Produces: a Docker image that serves the built static app on port 3002, proxying `/api/*` and `/health/*` to `$CENTRAL_API_UPSTREAM` (resolved from the container's environment at *start* time, not baked at build time — this is the fix for the exact bug class that caused the `CENTRAL_API_UPSTREAM` production incident earlier this session), with an nginx-level healthcheck.

- [ ] **Step 1: Write the nginx config template**

```nginx
# web-ui-v2/nginx.conf.template
server {
    listen 3002;
    root /usr/share/nginx/html;
    index index.html;

    location /api/ {
        proxy_pass ${CENTRAL_API_UPSTREAM}/api/;
        proxy_set_header Host $host;
    }

    location /health/ {
        proxy_pass ${CENTRAL_API_UPSTREAM}/health/;
        proxy_set_header Host $host;
    }

    location = /healthz {
        return 200 '{"status":"ok"}';
        add_header Content-Type application/json;
    }

    # SPA fallback: any other route serves index.html so TanStack Router's client-side
    # routing handles it (there is no server-side route matching for a Vite SPA).
    location / {
        try_files $uri $uri/ /index.html;
    }
}
```

- [ ] **Step 2: Write the entrypoint that resolves the template with `envsubst` at container start**

```bash
#!/bin/sh
# web-ui-v2/docker-entrypoint.sh
set -eu
envsubst '${CENTRAL_API_UPSTREAM}' < /etc/nginx/templates/nginx.conf.template > /etc/nginx/conf.d/default.conf
exec nginx -g "daemon off;"
```

Ensure this file has LF line endings (the `.gitattributes` rule this repo already added earlier this session, `*.sh text eol=lf`, covers this automatically — no extra step needed as long as this file's extension is `.sh`).

- [ ] **Step 3: Write the Dockerfile**

```dockerfile
# web-ui-v2/Dockerfile
FROM node:20-alpine AS builder
WORKDIR /app
COPY web-ui-v2/package.json web-ui-v2/package-lock.json ./
RUN npm ci
COPY web-ui-v2/ .
RUN npm run build

FROM nginx:alpine
COPY --from=builder /app/dist /usr/share/nginx/html
COPY web-ui-v2/nginx.conf.template /etc/nginx/templates/nginx.conf.template
COPY web-ui-v2/docker-entrypoint.sh /docker-entrypoint.sh
RUN chmod +x /docker-entrypoint.sh
ENV CENTRAL_API_UPSTREAM=http://127.0.0.1:8088
EXPOSE 3002
ENTRYPOINT ["/docker-entrypoint.sh"]
```

Note there is no build-time `--build-arg` for `CENTRAL_API_UPSTREAM` here at all — that's the point. It's a plain runtime `ENV` with a safe local default, resolved into the actual nginx config only when the container starts, via the entrypoint script. This is structurally immune to the "baked into a build-time asset" bug class that hit the Next.js proxy.

- [ ] **Step 4: Build locally and verify the proxy actually works before touching CI**

```bash
cd D:/ifascada/.claude/worktrees/cicd-central-webui-pipeline
docker build -f web-ui-v2/Dockerfile -t ifascada/web-ui-v2:dev-test .
docker run --rm -d --name web-ui-v2-test -p 3002:3002 --network ifascada_default -e CENTRAL_API_UPSTREAM=http://ifascada-central-server:8088 ifascada/web-ui-v2:dev-test
```

```bash
powershell -NoProfile -Command "(Invoke-WebRequest http://127.0.0.1:3002/api/tags/current -UseBasicParsing).StatusCode"
powershell -NoProfile -Command "(Invoke-WebRequest http://127.0.0.1:3002/healthz -UseBasicParsing).Content"
docker stop web-ui-v2-test
```

Expected: the `/api/tags/current` call returns `200` with real data (proving the runtime-resolved proxy reaches the real `central-server` container over the shared `ifascada_default` network), and `/healthz` returns the ok JSON.

- [ ] **Step 5: Commit**

```bash
git add web-ui-v2/Dockerfile web-ui-v2/nginx.conf.template web-ui-v2/docker-entrypoint.sh
git commit -m "feat(web-ui-v2): nginx-based Dockerfile with runtime-resolved reverse proxy"
```

---

### Task 16: docker-compose service and CI/CD pipeline

**Files:**
- Modify: `docker-compose.scada.yml` (add the `web-ui-v2` service)
- Create: `.github/workflows/web-ui-v2.yml`

**Interfaces:**
- Produces: a `web-ui-v2` service reachable on host port 3002 in the local dev stack, and a CI/CD pipeline that builds/tests/releases/deploys it to `.154:3002` on tags matching `webuiv2-v*`, following the exact same structure as `.github/workflows/web-ui.yml` (permissions, PATH fixes for `gh`, `--clobber` on release download, the SSH key path under `C:\ProgramData\ifascada-ci\.ssh`, the `production` environment gate) — this repo already solved every infra problem this pipeline will hit; copy the proven pattern rather than rediscovering it.

- [ ] **Step 1: Add the compose service**

```yaml
# docker-compose.scada.yml (append)
  web-ui-v2:
    build:
      context: .
      dockerfile: web-ui-v2/Dockerfile
    container_name: ifascada-web-ui-v2
    profiles: ["v2"]
    restart: unless-stopped
    environment:
      CENTRAL_API_UPSTREAM: http://central-server:8088
    ports:
      - "3002:3002"
```

- [ ] **Step 2: Verify it starts correctly in the local stack**

```bash
cd D:/ifascada/.claude/worktrees/cicd-central-webui-pipeline
docker compose -p ifascada -f docker-compose.scada.yml --profile central --profile seed --profile v2 up -d --build web-ui-v2
```

Confirm `docker ps --filter name=ifascada-web-ui-v2` shows it running, and repeat Task 15 Step 4's health/proxy checks against `http://127.0.0.1:3002` now via the compose network instead of a manually-run container.

- [ ] **Step 3: Write the CI/CD workflow, copying `.github/workflows/web-ui.yml`'s structure**

```yaml
# .github/workflows/web-ui-v2.yml
name: web-ui-v2

on:
  push:
    tags:
      - 'webuiv2-v*'

jobs:
  build:
    runs-on: [self-hosted, windows, ifascada]
    permissions:
      contents: write
    outputs:
      version: ${{ steps.version.outputs.version }}
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - name: Extract version from tag
        id: version
        shell: pwsh
        run: |
          $tag = "${{ github.ref_name }}"
          $version = $tag -replace '^webuiv2-v', ''
          "version=$version" >> $env:GITHUB_OUTPUT

      - name: Verify tag actually touches web-ui-v2
        shell: pwsh
        run: |
          $prevTag = git describe --tags --abbrev=0 --match "webuiv2-v*" "${{ github.ref_name }}^" 2>$null
          if (-not $prevTag) {
            Write-Output "No previous webuiv2-v* tag found -- treating this as the first release, nothing to compare against."
            exit 0
          }
          $changed = git diff --name-only "$prevTag" "${{ github.ref_name }}"
          $relevant = $changed | Where-Object { $_ -match '^web-ui-v2/' }
          if (-not $relevant) {
            Write-Error "Tag ${{ github.ref_name }} does not change anything relevant to web-ui-v2 since $prevTag -- refusing to build/deploy a no-op release."
            exit 1
          }
          Write-Output "Relevant changes since $prevTag :"
          $relevant | ForEach-Object { Write-Output " - $_" }

      - name: Install dependencies, test, and build
        shell: pwsh
        working-directory: web-ui-v2
        run: |
          npm ci
          npm test
          npm run build

      - name: Build Docker image
        shell: pwsh
        run: |
          docker build -f web-ui-v2/Dockerfile -t ifascada/web-ui-v2:${{ steps.version.outputs.version }} .

      - name: Save image to tar
        shell: pwsh
        run: |
          docker save ifascada/web-ui-v2:${{ steps.version.outputs.version }} -o web-ui-v2-${{ steps.version.outputs.version }}.tar

      - name: Publish GitHub Release
        uses: softprops/action-gh-release@v2
        with:
          tag_name: ${{ github.ref_name }}
          files: web-ui-v2-${{ steps.version.outputs.version }}.tar

  deploy:
    needs: build
    runs-on: [self-hosted, windows, ifascada]
    environment: production
    steps:
      - name: Ensure gh is on PATH
        shell: pwsh
        run: |
          "C:\Users\MATHIASHA\AppData\Local\gh-cli\bin" >> $env:GITHUB_PATH

      - name: Download release artifact
        shell: pwsh
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: |
          gh release download ${{ github.ref_name }} --pattern "*.tar" --dir . --clobber

      - name: Deploy to central host
        shell: pwsh
        run: |
          ./scripts/deploy-docker-service.ps1 `
            -Service "web-ui-v2" `
            -TargetHost "192.168.103.154" `
            -SshUser "ifa" `
            -SshKeyPath "C:\ProgramData\ifascada-ci\.ssh\ifascada_ci_deploy" `
            -ImageTarLocalPath "web-ui-v2-${{ needs.build.outputs.version }}.tar" `
            -NewImageRef "ifascada/web-ui-v2:${{ needs.build.outputs.version }}" `
            -HealthUrl "http://192.168.103.154:3002/healthz"
```

- [ ] **Step 4: Check `deploy-docker-service.ps1`'s `-Service` parameter accepts a new value**

```bash
grep -n "ValidateSet" D:/ifascada/.claude/worktrees/cicd-central-webui-pipeline/scripts/lib/DeployDockerService.ps1
```

That script's `-Service` parameter is currently `[ValidateSet("central-server", "web-ui")]` — add `"web-ui-v2"` to that set (a one-line change) before this workflow's deploy step can run. Also confirm the remote `.154` host has a `WEB_UI_V2_IMAGE`-style `.env` entry and a `web-ui-v2` service block in whatever `docker-compose.yml` actually runs in production there (not the local `docker-compose.scada.yml` — the two have already diverged once this session when the incident was investigated; check the real file on `.154` via SSH before assuming it matches this repo's compose file).

- [ ] **Step 5: Commit**

```bash
git add docker-compose.scada.yml .github/workflows/web-ui-v2.yml scripts/lib/DeployDockerService.ps1
git commit -m "feat(ci): add web-ui-v2 compose service and CI/CD pipeline"
```

---

### Task 17: First real deploy — infra-only smoke release

**Files:** none (this task tags and ships whatever Tasks 0–16 produced).

This mirrors what this session already learned the hard way with the original `web-ui` pipeline: validate the *infrastructure* (build, Docker image, nginx proxy, CI/CD, production environment approval gate, health check) end-to-end before trusting it with real page content changes. By this point in the plan, real Live/History pages already exist (Tasks 9/12), so this is simply the first real tag-triggered deploy, not a throwaway page.

- [ ] **Step 1: Tag and push**

```bash
cd D:/ifascada/.claude/worktrees/cicd-central-webui-pipeline
git tag webuiv2-v0.1.0
git push origin webuiv2-v0.1.0
```

- [ ] **Step 2: Watch the build job**, confirm `npm test` (Task 0's harness) runs as part of CI and passes, confirm the Docker image builds, confirm the GitHub Release is published with the `.tar` artifact.

- [ ] **Step 3: Approve the deploy** in the `production` environment when the workflow pauses for it.

- [ ] **Step 4: Verify independently** (the same pattern used for every deploy this session): health check via `Invoke-WebRequest http://192.168.103.154:3002/healthz`, a real `/api/tags/current` call through the proxy, `docker ps`/`docker images` on `.154` confirming the container is running the new tag, and a Playwright screenshot of `http://192.168.103.154:3002/live` confirming it renders — all without touching the existing `web-ui:3001` container's traffic.

- [ ] **Step 5: Confirm the old `web-ui` is completely unaffected**

```bash
powershell -NoProfile -Command "(Invoke-WebRequest http://192.168.103.154:3001/api/health -UseBasicParsing).Content"
```

Expected: still `200 {"status":"ok"}`, unchanged — this is the whole point of the parallel-rollout strategy, and it's worth confirming explicitly rather than assuming.

This plan's scope ends here. The cutover decision (pointing real operators at :3002, retiring `web-ui:3001`) is explicitly out of scope per the spec — a future, separate decision once `web-ui-v2` has been used and trusted enough to warrant it.
