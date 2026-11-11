import { useEffect, useState } from "react";
import { useT } from "../i18n";
import { Button } from "./Button";

/**
 * Copies a text to the clipboard, and says so for a moment: "Copied" goes back to "Copy" so a second
 * copy gives feedback again, and the announcement is polite so a screen reader hears it.
 */
export function CopyButton({ text, label }: { text: string; label: string }) {
  const t = useT();
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
        {t("Copy")}
      </Button>
      <span role="status" className="muted">
        {copied ? t("Copied.") : ""}
      </span>
    </>
  );
}
