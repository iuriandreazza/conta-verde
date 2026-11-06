import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it } from "vitest";
import { Sheet } from "./Sheet";

function Page() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button onClick={() => setOpen(true)}>Novo</button>
      <Sheet open={open} title="Novo registro" onClose={() => setOpen(false)}>
        <label>
          Nome
          <input />
        </label>
        <button>Salvar</button>
      </Sheet>
      <button>Depois</button>
    </>
  );
}

describe("Sheet", () => {
  it("is not there until it is asked for, and then is a named dialog with the first field focused", async () => {
    render(<Page />);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Novo" }));

    expect(screen.getByRole("dialog", { name: "Novo registro" })).toHaveAttribute("aria-modal", "true");
    expect(screen.getByLabelText("Nome")).toHaveFocus();
  });

  it("keeps the Tab key inside, going round from the last control to the first", async () => {
    render(<Page />);
    await userEvent.click(screen.getByRole("button", { name: "Novo" }));

    await userEvent.tab();
    expect(screen.getByRole("button", { name: "Salvar" })).toHaveFocus();
    await userEvent.tab();
    expect(screen.getByRole("button", { name: "Fechar" })).toHaveFocus();
    await userEvent.tab({ shift: true });
    expect(screen.getByRole("button", { name: "Salvar" })).toHaveFocus();
  });

  it("closes on Escape, and the focus goes back to what opened it", async () => {
    render(<Page />);
    const opener = screen.getByRole("button", { name: "Novo" });
    await userEvent.click(opener);

    await userEvent.keyboard("{Escape}");

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(opener).toHaveFocus();
  });

  it("closes from its own button", async () => {
    render(<Page />);
    await userEvent.click(screen.getByRole("button", { name: "Novo" }));

    await userEvent.click(screen.getByRole("button", { name: "Fechar" }));

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
});
