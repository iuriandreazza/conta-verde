import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ListFooter } from "./ListFooter";

const noun: [string, string] = ["conta", "contas"];

describe("ListFooter", () => {
  it("counts the rows in the right number and offers nothing when the server has no more", () => {
    render(<ListFooter count={1} noun={noun} hasMore={false} loadingMore={false} onLoadMore={vi.fn()} />);

    expect(screen.getByRole("status")).toHaveTextContent("Mostrando 1 conta");
    expect(screen.queryByRole("button", { name: "Carregar mais" })).not.toBeInTheDocument();
  });

  it("says there is more and brings it on request", async () => {
    const onLoadMore = vi.fn();
    render(<ListFooter count={25} noun={noun} hasMore loadingMore={false} onLoadMore={onLoadMore} />);

    expect(screen.getByRole("status")).toHaveTextContent("Mostrando 25 contas, e há mais");
    await userEvent.click(screen.getByRole("button", { name: "Carregar mais" }));

    expect(onLoadMore).toHaveBeenCalledTimes(1);
  });
});
