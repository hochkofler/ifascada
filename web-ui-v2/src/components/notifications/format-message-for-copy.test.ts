/**
 * Cosechado de libs/notifications/src/format-message-for-copy.test.ts de ifahub. Unica adaptacion: el runner
 * (node:test + node:assert -> vitest, que es el de este proyecto) y el formateador de fecha (formatDateTime -> formatServerDateTime local).
 */
import { expect, test } from "vitest";
import { formatServerDateTime } from "@/lib/datetime";
import { formatMessageForCopy } from "./format-message-for-copy";

const labels = {
  title: "Titulo",
  description: "Descripcion",
  docNum: "Documento",
  code: "Codigo",
  httpStatus: "HTTP Status",
  source: "Origen",
  timestamp: "Fecha",
  correlationId: "ID de seguimiento",
};

test("solo incluye titulo y fecha cuando el resto de los campos no estan presentes", () => {
  const timestamp = "2026-07-07T10:00:00";
  const text = formatMessageForCopy(
    { id: "1", level: "error", title: "Error al guardar", timestamp },
    labels
  );
  expect(text).toBe(`Titulo: Error al guardar\nFecha: ${formatServerDateTime(timestamp)}`);
});

test("incluye todos los campos presentes, en orden, y omite los ausentes", () => {
  const timestamp = "2026-07-07T10:00:00";
  const text = formatMessageForCopy(
    {
      id: "1",
      level: "error",
      title: "No se pudo crear la orden",
      description: "Stock insuficiente para el articulo A0001",
      docNum: 1234,
      code: "SAP_ERROR",
      httpStatus: 400,
      source: "sales/orders",
      correlationId: "abc-123",
      timestamp,
    },
    labels
  );
  expect(text).toBe(
    [
      "Titulo: No se pudo crear la orden",
      "Descripcion: Stock insuficiente para el articulo A0001",
      "Documento: #1234",
      "Codigo: SAP_ERROR",
      "HTTP Status: 400",
      "Origen: sales/orders",
      `Fecha: ${formatServerDateTime(timestamp)}`,
      "ID de seguimiento: abc-123",
    ].join("\n")
  );
});

test("trata la descripcion vacia como ausente", () => {
  const timestamp = "2026-07-07T10:00:00";
  const text = formatMessageForCopy(
    { id: "1", level: "info", title: "Ok", description: "", timestamp },
    labels
  );
  expect(text).toBe(`Titulo: Ok\nFecha: ${formatServerDateTime(timestamp)}`);
});
