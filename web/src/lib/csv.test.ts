import { describe, expect, it } from "vitest";
import { toCsv } from "./csv";

describe("toCsv", () => {
  it("starts with the byte-order mark and separates with semicolons and CRLF", () => {
    expect(toCsv(["Nome", "Total"], [["Mercado", "12,5"]])).toBe("﻿Nome;Total\r\nMercado;12,5\r\n");
  });

  it("quotes what holds a separator, a quote or a line break", () => {
    expect(toCsv(["a"], [["x;y"], ['diz "oi"'], ["duas\nlinhas"]])).toBe(
      '﻿a\r\n"x;y"\r\n"diz ""oi"""\r\n"duas\nlinhas"\r\n',
    );
  });

  it("keeps a spreadsheet from running a cell as a formula, but leaves negative numbers alone", () => {
    const text = toCsv(["a"], [["=1+1"], ["+55 11"], ["@x"], ["-cmd"], ["-10.5"], ["-3"]]);
    expect(text).toBe("﻿a\r\n'=1+1\r\n'+55 11\r\n'@x\r\n'-cmd\r\n-10.5\r\n-3\r\n");
  });
});
