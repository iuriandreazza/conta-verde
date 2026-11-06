import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it } from "vitest";
import { BackButton } from "./BackButton";
import { Sheet, useSheetDirty } from "./Sheet";

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

describe("Sheet with a form that arrives late", () => {
  it("puts the focus in the first field when it shows up after the sheet, and keeps Shift+Tab inside", async () => {
    function Late() {
      const [ready, setReady] = useState(false);
      return (
        <>
          <button onClick={() => setReady(true)}>Chegou</button>
          <Sheet open title="Novo" onClose={() => undefined}>
            {ready && (
              <label>
                Descrição
                <input />
              </label>
            )}
          </Sheet>
        </>
      );
    }
    render(<Late />);
    expect(screen.getByRole("dialog")).toHaveFocus();

    // Shift+Tab from the panel itself goes to the last control, not out to the page behind.
    await userEvent.tab({ shift: true });
    expect(screen.getByRole("button", { name: "Fechar" })).toHaveFocus();
  });

  it("does not let the page behind scroll while it is open, and gives that back", async () => {
    render(<Page />);
    await userEvent.click(screen.getByRole("button", { name: "Novo" }));
    expect(document.body.style.overflow).toBe("hidden");

    await userEvent.keyboard("{Escape}");

    expect(document.body.style.overflow).toBe("");
  });
});

describe("Sheet and what was typed", () => {
  function Typing() {
    const [open, setOpen] = useState(true);
    return (
      <>
        <button onClick={() => setOpen(true)}>Reabrir</button>
        <Sheet open={open} title="Novo" onClose={() => setOpen(false)}>
          <label>
            Nome
            <input />
          </label>
          <BackButton onClick={() => setOpen(false)} />
        </Sheet>
      </>
    );
  }

  it("closes at once when nothing was typed", async () => {
    render(<Typing />);

    await userEvent.click(screen.getByRole("button", { name: "Fechar" }));

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("asks before throwing typed text away, and lets the person keep editing", async () => {
    render(<Typing />);
    await userEvent.type(screen.getByLabelText("Nome"), "Aluguel");

    await userEvent.keyboard("{Escape}");
    expect(screen.getByRole("alertdialog", { name: "Descartar edições?" })).toBeInTheDocument();
    expect(screen.getByRole("dialog", { name: "Novo" })).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Voltar e editar" }));
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(screen.getByLabelText("Nome")).toHaveValue("Aluguel");
  });

  it("closes once the person confirms, whichever way out they took", async () => {
    render(<Typing />);
    await userEvent.type(screen.getByLabelText("Nome"), "Aluguel");

    await userEvent.click(screen.getByRole("button", { name: "Voltar" }));
    await userEvent.click(screen.getByRole("button", { name: "Descartar e fechar" }));

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("starts clean when it opens again", async () => {
    render(<Typing />);
    await userEvent.type(screen.getByLabelText("Nome"), "Aluguel");
    await userEvent.click(screen.getByRole("button", { name: "Fechar" }));
    await userEvent.click(screen.getByRole("button", { name: "Descartar e fechar" }));

    await userEvent.click(screen.getByRole("button", { name: "Reabrir" }));
    await userEvent.click(screen.getByRole("button", { name: "Fechar" }));

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("keeps Tab and Escape inside the question, and gives the focus back to the field", async () => {
    render(<Typing />);
    const field = screen.getByLabelText("Nome");
    await userEvent.type(field, "Aluguel");
    await userEvent.keyboard("{Escape}");
    expect(screen.getByRole("button", { name: "Voltar e editar" })).toHaveFocus();

    await userEvent.tab();
    expect(screen.getByRole("button", { name: "Descartar e fechar" })).toHaveFocus();
    await userEvent.tab();
    expect(screen.getByRole("button", { name: "Voltar e editar" })).toHaveFocus();

    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    await waitFor(() => expect(field).toHaveFocus());
  });

  it("neither closes nor asks while a save is on its way", async () => {
    function Saving() {
      return (
        <Sheet open title="Novo" locked onClose={() => undefined}>
          <input aria-label="Nome" />
        </Sheet>
      );
    }
    render(<Saving />);
    await userEvent.type(screen.getByLabelText("Nome"), "Aluguel");

    await userEvent.click(screen.getByRole("button", { name: "Fechar" }));
    await userEvent.keyboard("{Escape}");

    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(screen.getByRole("dialog", { name: "Novo" })).toBeInTheDocument();
  });

  it("counts a click-only change (a row added) as something to lose", async () => {
    function Rows() {
      const markDirty = useSheetDirty();
      return <button onClick={markDirty}>Adicionar linha</button>;
    }
    render(
      <Sheet open title="Novo" onClose={() => undefined}>
        <Rows />
      </Sheet>,
    );

    await userEvent.click(screen.getByRole("button", { name: "Adicionar linha" }));
    await userEvent.click(screen.getByRole("button", { name: "Fechar" }));

    expect(screen.getByRole("alertdialog", { name: "Descartar edições?" })).toBeInTheDocument();
  });

  it("does not re-render a controlled field in the middle of a change", async () => {
    function Choice() {
      useSheetDirty();
      const [value, setValue] = useState("a");
      return (
        <select aria-label="Escolha" value={value} onChange={(event) => setValue(event.target.value)}>
          <option value="a">A</option>
          <option value="b">B</option>
        </select>
      );
    }
    render(
      <Sheet open title="Novo" onClose={() => undefined}>
        <Choice />
      </Sheet>,
    );

    await userEvent.selectOptions(screen.getByLabelText("Escolha"), "b");

    expect(screen.getByLabelText("Escolha")).toHaveValue("b");
  });
});
