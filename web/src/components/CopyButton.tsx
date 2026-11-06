import { useEffect, useState } from "react";
import { Button } from "./Button";

/**
 * Copies a text to the clipboard, and says so for a moment: "Copiado" goes back to "Copiar" so a second
 * copy gives feedback again, and the announcement is polite so a screen reader hears it.
 */
export function CopyButton({ text, label }: { text: string; label: string }) {
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!copied) {
      return;
    }
    const timer = setTimeout(() => setCopied(false), 2000);
    return () => clearTimeout(timer);
  }, [copied]);

  async function copy() {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
    } catch {
      // No clipboard where this runs (or no permission): the text stays on the page to be selected.
      setCopied(false);
    }
  }

  return (
    <>
      <Button type="button" variant="ghost" className="btn-small" aria-label={label} onClick={() => void copy()}>
        Copiar
      </Button>
      <span role="status" className="muted">
        {copied ? "Copiado." : ""}
      </span>
    </>
  );
}
