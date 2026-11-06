import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ListRow } from "./ListRow";

function rowWith(tone?: "critical") {
  render(
    <table>
      <tbody>
        <ListRow index={0} highlighted={false} tone={tone}>
          <td>Aluguel</td>
        </ListRow>
      </tbody>
    </table>,
  );
  return screen.getByRole("row");
}

describe("ListRow", () => {
  it("marks a row that needs attention and leaves the others alone", () => {
    expect(rowWith("critical")).toHaveClass("row-critical");
  });

  it("has no mark by default", () => {
    expect(rowWith()).not.toHaveClass("row-critical");
  });
});
