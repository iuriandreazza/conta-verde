import { DEFAULT_LANGUAGE, translate, type Translate } from "../i18n";

/** The largest file an entry takes (ADR 0036): the API refuses more, this spares the upload. */
export const MAX_ATTACHMENT_BYTES = 10 * 1024 * 1024;
export const MAX_ENTRY_ATTACHMENTS = 20;

/** What the file picker offers; the API holds the real list of types. */
export const ATTACHMENT_ACCEPT = ".pdf,.png,.jpg,.jpeg,.webp,.txt,.csv,.xml";

const TYPES_BY_EXTENSION: Record<string, string> = {
  pdf: "application/pdf",
  png: "image/png",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  webp: "image/webp",
  txt: "text/plain",
  csv: "text/csv",
  xml: "application/xml",
};

/**
 * The type to send for a file. The extension decides, since it is what the picker offered: a browser
 * on Windows with Excel installed calls a `.csv` `application/vnd.ms-excel`, and a file it does not
 * know has no type at all.
 */
export function contentTypeOf(file: { name: string; type: string }): string {
  const extension = file.name.split(".").pop()?.toLowerCase() ?? "";
  return TYPES_BY_EXTENSION[extension] ?? (file.type || "application/octet-stream");
}

/** `13 B`, `2,5 KB`, `1,2 MB`: the size as a person reads it. */
export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const [value, unit] = bytes < 1024 * 1024 ? [bytes / 1024, "KB"] : [bytes / (1024 * 1024), "MB"];
  return `${value.toFixed(1).replace(".", ",").replace(/,0$/, "")} ${unit}`;
}

/** Why a file will not be sent, in words for the person, or `null` when it may be. */
export function fileProblem(
  file: { name: string; size: number },
  t: Translate = (key, params) => translate(DEFAULT_LANGUAGE, key, params),
): string | null {
  if (file.size === 0) return t("{name} is empty.", { name: file.name });
  if (file.size > MAX_ATTACHMENT_BYTES) {
    return t("{name} is over {size}, the limit for each file.", { name: file.name, size: formatSize(MAX_ATTACHMENT_BYTES) });
  }
  return null;
}
