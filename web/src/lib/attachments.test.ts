import { describe, expect, it } from "vitest";
import { MAX_ATTACHMENT_BYTES, fileProblem, formatSize } from "./attachments";

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
