import { describe, expect, it } from "vitest";
import { fitted, monthSpan } from "./reports";

describe("monthSpan", () => {
  it("is the whole month of the day", () => {
    expect(monthSpan("2026-11-05")).toEqual({ from: "2026-11-01", to: "2026-11-30" });
    expect(monthSpan("2026-12-31")).toEqual({ from: "2026-12-01", to: "2026-12-31" });
    expect(monthSpan("2028-02-10")).toEqual({ from: "2028-02-01", to: "2028-02-29" });
    expect(monthSpan("2026-02-10")).toEqual({ from: "2026-02-01", to: "2026-02-28" });
  });
});

describe("fitted", () => {
  it("cuts a long name and leaves a short one", () => {
    expect(fitted("Vendas")).toBe("Vendas");
    expect(fitted("Fornecedores de matéria-prima")).toBe("Fornecedores de m…");
    expect(fitted("Fornecedores de matéria-prima")).toHaveLength(18);
  });
});
