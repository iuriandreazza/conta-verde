import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { PageHeader } from "./PageHeader";

describe("PageHeader", () => {
  it("puts what the screen is for behind an (i) beside the title, and keeps facts about the page in sight", async () => {
    render(<PageHeader title="Contratos" description="Cobranças que se repetem." subtitle="Padaria para Maria" />);

    expect(screen.getByRole("heading", { name: "Contratos" })).toBeInTheDocument();
    expect(screen.getByText("Padaria para Maria")).toBeInTheDocument();
    expect(screen.queryByText("Cobranças que se repetem.")).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Sobre esta tela: Contratos" }));

    expect(screen.getByRole("tooltip")).toHaveTextContent("Cobranças que se repetem.");
  });

  it("shows no (i) when there is nothing to explain", () => {
    render(<PageHeader title="Painel" />);

    expect(screen.queryByRole("button", { name: /Sobre esta tela/ })).not.toBeInTheDocument();
  });
});
