import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { Button } from "./button";

describe("Button (vendored from @ifahub/ui)", () => {
  it("renders its children and responds to variant prop", () => {
    render(<Button variant="destructive">Eliminar</Button>);
    const btn = screen.getByRole("button", { name: "Eliminar" });
    expect(btn).toBeInTheDocument();
    expect(btn).toHaveAttribute("data-variant", "destructive");
  });
});
