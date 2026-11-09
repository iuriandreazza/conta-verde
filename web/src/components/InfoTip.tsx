import { AnimatePresence, motion } from "motion/react";
import { useEffect, useId, useRef, useState, type ReactNode } from "react";

/**
 * The explanation of a card behind an (i) button, so the card shows what it is for and the
 * how-to is one hover, tap or key press away. Hover shows it, a click pins it, Escape or a click
 * outside puts it away; the pointer can move onto the text without it vanishing.
 */
export function InfoTip({
  label,
  align = "end",
  children,
}: {
  label: string;
  /** Which edge of the button the text lines up with: `end` where the (i) sits at the right of a card. */
  align?: "start" | "end";
  children: ReactNode;
}) {
  const [pinned, setPinned] = useState(false);
  const [hovered, setHovered] = useState(false);
  const root = useRef<HTMLSpanElement>(null);
  const id = useId();
  const visible = pinned || hovered;

  useEffect(() => {
    if (!visible) return;
    function away(event: PointerEvent) {
      if (root.current?.contains(event.target as Node)) return;
      setPinned(false);
      setHovered(false);
    }
    document.addEventListener("pointerdown", away);
    return () => document.removeEventListener("pointerdown", away);
  }, [visible]);

  return (
    <span
      className="info-tip"
      ref={root}
      onPointerEnter={(event) => event.pointerType === "mouse" && setHovered(true)}
      onPointerLeave={() => setHovered(false)}
      onKeyDown={(event) => {
        if (event.key === "Escape" && visible) {
          event.stopPropagation();
          setPinned(false);
          setHovered(false);
        }
      }}
    >
      <button
        type="button"
        className="info-tip-button"
        aria-label={label}
        aria-expanded={pinned}
        aria-describedby={visible ? id : undefined}
        onClick={() => setPinned(!pinned)}
      >
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <circle cx="12" cy="12" r="10" />
          <path d="M12 16v-4" />
          <path d="M12 8h.01" />
        </svg>
      </button>
      <AnimatePresence initial={false}>
        {visible && (
          <motion.div
            id={id}
            role="tooltip"
            className={`info-tip-text info-tip-${align}`}
            initial={{ opacity: 0, y: -4 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -4 }}
            transition={{ duration: 0.14 }}
          >
            {children}
          </motion.div>
        )}
      </AnimatePresence>
    </span>
  );
}
