import { describe, expect, it } from "vitest";
import {
  describeInstallments,
  describeInstallmentsText,
  showHundredths,
  splitEvenly,
  sumOfPercents,
} from "./installments";

describe("installments", () => {
  it("splits 100% into equal shares with the remainder on the last", () => {
    expect(splitEvenly(1)).toEqual(["100"]);
    expect(splitEvenly(2)).toEqual(["50", "50"]);
    expect(splitEvenly(3)).toEqual(["33.33", "33.33", "33.34"]);
    expect(splitEvenly(6)).toEqual(["16.66", "16.66", "16.66", "16.66", "16.66", "16.7"]);
    expect(splitEvenly(0)).toEqual([]);
    // However many, they add up to exactly 100 and never to 99.99999999999999.
    for (let count = 1; count <= 24; count++) {
      expect(sumOfPercents(splitEvenly(count))).toBe(10_000);
    }
  });

  it("adds up what a person typed in hundredths, or says it is not a share", () => {
    expect(sumOfPercents(["33,33", "33.33", "33,34"])).toBe(10_000);
    expect(sumOfPercents(["0,01", "99,99"])).toBe(10_000);
    expect(sumOfPercents(["50"])).toBe(5_000);
    for (const bad of ["", "abc", "33,333", "1e2", "-5", "50 %"]) {
      expect(sumOfPercents(["50", bad]), bad).toBeNull();
    }
  });

  it("tells a difference the way a person writes it", () => {
    expect(showHundredths(1)).toBe("0,01");
    expect(showHundredths(-250)).toBe("2,5");
    expect(showHundredths(10_000)).toBe("100");
  });

  it("describes the installments in words", () => {
    expect(describeInstallments([{ days: 0, percent: "50" }, { days: 30, percent: "50" }])).toBe(
      "à vista 50% · 30 dias 50%",
    );
    expect(describeInstallments([{ days: 1, percent: "33.33" }])).toBe("1 dia 33,33%");
    expect(describeInstallmentsText("0:40.5,30:59.5")).toBe("à vista 40,5% · 30 dias 59,5%");
  });
});
