import { describe, it, expect } from "vitest";
import { dedupeByCode } from "./context-options";

describe("dedupeByCode", () => {
  // Caso real: /api/context/cells?site=plant-a devuelve dos celdas `cell-main` con nombres
  // distintos, y React tiraba "Encountered two children with the same key".
  it("descarta la segunda opcion con el mismo codigo", () => {
    const out = dedupeByCode([
      { code: "cell-main", name: "Cell Main" },
      { code: "cell-main", name: "main" },
    ]);
    expect(out).toEqual([{ code: "cell-main", name: "Cell Main" }]);
  });

  it("conserva la primera, no la ultima", () => {
    const out = dedupeByCode([
      { code: "x", name: "primera" },
      { code: "x", name: "segunda" },
    ]);
    expect(out[0].name).toBe("primera");
  });

  it("deja intacta una lista sin duplicados", () => {
    const input = [
      { code: "a", name: "A" },
      { code: "b", name: "B" },
    ];
    expect(dedupeByCode(input)).toEqual(input);
  });

  it("tolera la lista vacia", () => {
    expect(dedupeByCode([])).toEqual([]);
  });
});
