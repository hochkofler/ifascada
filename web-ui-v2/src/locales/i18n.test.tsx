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
