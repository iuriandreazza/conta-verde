import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { BarChart } from "./BarChart";

const series = [
  { name: "A receber", color: "blue" },
  { name: "A pagar", color: "orange" },
];

describe("BarChart", () => {
  it("prints no amount beside a bar that has no length, but still names it for assistive technology", () => {
    render(
      <BarChart
        label="Por categoria"
        series={series}
        groups={[{ label: "Vendas", values: ["120.50", "0"] }]}
      />,
    );

    expect(screen.getByText("R$ 120,50")).toBeInTheDocument();
    expect(screen.queryByText("R$ 0,00")).not.toBeInTheDocument();
    expect(screen.getByRole("group", { name: /Vendas: A receber R\$ 120,50, A pagar R\$ 0,00/ })).toBeInTheDocument();
  });
});
