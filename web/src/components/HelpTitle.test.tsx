import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { HelpTitle } from "./HelpTitle";

describe("HelpTitle", () => {
  it("names the heading by its level and the (i) by the heading it explains", async () => {
    render(
      <HelpTitle level={2} title="Extrato">
        <p>A base é o valor líquido.</p>
      </HelpTitle>,
    );

    expect(screen.getByRole("heading", { level: 2, name: "Extrato" })).toBeInTheDocument();
    expect(screen.queryByText("A base é o valor líquido.")).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Sobre Extrato" }));

    expect(screen.getByRole("tooltip")).toHaveTextContent("A base é o valor líquido.");
  });

  it("is a level 3 heading with the id a form section points at, unless said otherwise", () => {
    render(
      <HelpTitle id="rule-who" title="A quem vale">
        <p>Texto.</p>
      </HelpTitle>,
    );

    expect(screen.getByRole("heading", { level: 3, name: "A quem vale" })).toHaveAttribute("id", "rule-who");
  });
});
