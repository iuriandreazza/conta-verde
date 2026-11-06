import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";
import { MenuButton } from "./MenuButton";

function setup(onSelect = vi.fn()) {
  render(
    <MemoryRouter>
      <MenuButton
        label="Novo"
        items={[
          { key: "a", label: "Primeiro", onSelect },
          { key: "b", label: "Bloqueado", disabled: true, onSelect },
          { key: "c", label: "Terceiro", onSelect },
        ]}
      />
      <button>Outro</button>
    </MemoryRouter>,
  );
  return { onSelect, trigger: screen.getByRole("button", { name: "Novo" }) };
}

describe("MenuButton", () => {
  it("opens with the first choice focused and says so to assistive technology", async () => {
    const { trigger } = setup();
    expect(trigger).toHaveAttribute("aria-expanded", "false");

    await userEvent.click(trigger);

    expect(trigger).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("menuitem", { name: "Primeiro" })).toHaveFocus();
  });

  it("moves with the arrows, skipping what is disabled, and wraps around", async () => {
    const { trigger } = setup();
    await userEvent.click(trigger);

    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getByRole("menuitem", { name: "Terceiro" })).toHaveFocus();
    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getByRole("menuitem", { name: "Primeiro" })).toHaveFocus();
    await userEvent.keyboard("{End}");
    expect(screen.getByRole("menuitem", { name: "Terceiro" })).toHaveFocus();
  });

  it("closes on Escape and gives the focus back to the button", async () => {
    const { trigger } = setup();
    await userEvent.click(trigger);

    await userEvent.keyboard("{Escape}");

    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
  });

  it("closes when the person clicks anywhere else", async () => {
    const { trigger } = setup();
    await userEvent.click(trigger);

    await userEvent.click(screen.getByRole("button", { name: "Outro" }));

    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("does the chosen thing once and closes, and does nothing for a disabled choice", async () => {
    const { trigger, onSelect } = setup();
    await userEvent.click(trigger);
    await userEvent.click(screen.getByRole("menuitem", { name: "Bloqueado" }));
    expect(onSelect).not.toHaveBeenCalled();
    expect(screen.getByRole("menu")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("menuitem", { name: "Terceiro" }));

    expect(onSelect).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
  });
});
