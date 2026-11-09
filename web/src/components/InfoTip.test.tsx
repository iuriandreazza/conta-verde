import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { InfoTip } from "./InfoTip";

function renderTip() {
  return render(
    <div>
      <InfoTip label="Como importar">
        <p>A primeira linha dá o nome das colunas.</p>
      </InfoTip>
      <button type="button">Outro</button>
    </div>,
  );
}

describe("InfoTip", () => {
  it("keeps the text out of sight until the (i) is pressed, and ties it to the button while shown", async () => {
    renderTip();
    const button = screen.getByRole("button", { name: "Como importar" });
    expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
    expect(button).toHaveAttribute("aria-expanded", "false");

    await userEvent.click(button);

    const tip = screen.getByRole("tooltip");
    expect(tip).toHaveTextContent("A primeira linha dá o nome das colunas.");
    expect(button).toHaveAttribute("aria-expanded", "true");
    expect(button).toHaveAttribute("aria-describedby", tip.id);
  });

  it("is put away by a second press, by Escape and by a click elsewhere", async () => {
    renderTip();
    const button = screen.getByRole("button", { name: "Como importar" });

    const gone = () => waitFor(() => expect(screen.queryByRole("tooltip")).not.toBeInTheDocument());

    // Pressed again, with the pointer taken off (a pointer still over the button keeps showing it).
    await userEvent.click(button);
    await userEvent.click(button);
    await userEvent.unhover(button);
    await gone();

    await userEvent.click(button);
    await userEvent.keyboard("{Escape}");
    await gone();

    await userEvent.click(button);
    await userEvent.click(screen.getByRole("button", { name: "Outro" }));
    await gone();
  });

  it("opens with the keyboard", async () => {
    renderTip();
    await userEvent.tab();
    expect(screen.getByRole("button", { name: "Como importar" })).toHaveFocus();

    await userEvent.keyboard("{Enter}");

    expect(screen.getByRole("tooltip")).toBeInTheDocument();
  });
});
