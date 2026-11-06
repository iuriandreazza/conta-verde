import { describe, expect, it } from "vitest";
import { MAX_ATTACHMENT_BYTES, contentTypeOf, fileProblem, formatSize } from "./attachments";

describe("formatSize", () => {
  it("reads as bytes, kilobytes or megabytes with a decimal comma and no needless zero", () => {
    expect(formatSize(13)).toBe("13 B");
    expect(formatSize(1023)).toBe("1023 B");
    expect(formatSize(1024)).toBe("1 KB");
    expect(formatSize(2560)).toBe("2,5 KB");
    expect(formatSize(1024 * 1024)).toBe("1 MB");
    expect(formatSize(MAX_ATTACHMENT_BYTES)).toBe("10 MB");
    expect(formatSize(1.2 * 1024 * 1024)).toBe("1,2 MB");
  });
});

describe("fileProblem", () => {
  it("refuses an empty file and one past the limit, and says which", () => {
    expect(fileProblem({ name: "a.pdf", size: 1 })).toBeNull();
    expect(fileProblem({ name: "a.pdf", size: MAX_ATTACHMENT_BYTES })).toBeNull();
    expect(fileProblem({ name: "a.pdf", size: 0 })).toBe("a.pdf está vazio.");
    expect(fileProblem({ name: "a.pdf", size: MAX_ATTACHMENT_BYTES + 1 })).toBe(
      "a.pdf passa de 10 MB, o limite de cada arquivo.",
    );
  });
});

describe("contentTypeOf", () => {
  it("goes by the extension, and falls back on what the browser said", () => {
    expect(contentTypeOf({ name: "Extrato.CSV", type: "application/vnd.ms-excel" })).toBe("text/csv");
    expect(contentTypeOf({ name: "foto.jpeg", type: "" })).toBe("image/jpeg");
    expect(contentTypeOf({ name: "a.b.pdf", type: "" })).toBe("application/pdf");
    expect(contentTypeOf({ name: "arquivo.bin", type: "application/zip" })).toBe("application/zip");
    expect(contentTypeOf({ name: "semextensao", type: "" })).toBe("application/octet-stream");
  });
});
