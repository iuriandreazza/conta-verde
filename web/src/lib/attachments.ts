/** The largest file an entry takes (ADR 0036): the API refuses more, this spares the upload. */
export const MAX_ATTACHMENT_BYTES = 10 * 1024 * 1024;
export const MAX_ENTRY_ATTACHMENTS = 20;

/** What the file picker offers; the API holds the real list of types. */
export const ATTACHMENT_ACCEPT = ".pdf,.png,.jpg,.jpeg,.webp,.txt,.csv,.xml";

/** `13 B`, `2,5 KB`, `1,2 MB`: the size as a person reads it. */
export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const [value, unit] = bytes < 1024 * 1024 ? [bytes / 1024, "KB"] : [bytes / (1024 * 1024), "MB"];
  return `${value.toFixed(1).replace(".", ",").replace(/,0$/, "")} ${unit}`;
}

/** Why a file will not be sent, in words for the person, or `null` when it may be. */
export function fileProblem(file: { name: string; size: number }): string | null {
  if (file.size === 0) return `${file.name} está vazio.`;
  if (file.size > MAX_ATTACHMENT_BYTES) {
    return `${file.name} passa de ${formatSize(MAX_ATTACHMENT_BYTES)}, o limite de cada arquivo.`;
  }
  return null;
}
