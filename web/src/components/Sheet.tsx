import { AnimatePresence, motion } from "motion/react";
import { createContext, useCallback, useContext, useEffect, useId, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { useT } from "../i18n";
import { Button } from "./Button";
import { XIcon } from "./icons/x";

/** Closing, or going back, asks first when something was typed: the way out of the sheet runs through it. */
const SheetGuard = createContext<{ guard: (action: () => void) => void; markDirty: () => void }>({
  guard: (action) => action(),
  markDirty: () => undefined,
});

export function useSheetGuard() {
  return useContext(SheetGuard).guard;
}

/** For a change that is a click, not typing (a row added, a row removed): it counts as something to lose. */
export function useSheetDirty() {
  return useContext(SheetGuard).markDirty;
}

interface SheetProps {
  open: boolean;
  title: string;
  onClose: () => void;
  /** Only the close button closes it: for what must be read first, a stray click or key must not. */
  explicitClose?: boolean;
  /** A save is on its way: nothing closes the sheet, and nothing asks about it, until it ends. */
  locked?: boolean;
  children: ReactNode;
}

const FOCUSABLE = 'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/**
 * A bottom sheet: it rises over the page that is open, so a record is added or changed without
 * leaving the list. A dialog for assistive technology: the focus goes in, stays in until it closes,
 * Escape and the shade close it, and the focus returns to what opened it.
 */
export function Sheet({ open, title, onClose, explicitClose = false, locked = false, children }: SheetProps) {
  const t = useT();
  const titleId = useId();
  const panelRef = useRef<HTMLDivElement>(null);
  const opener = useRef<HTMLElement | null>(null);
  // A ref, not state: typing must not re-render what the sheet holds (a controlled field would lose its change).
  const dirty = useRef(false);
  // Each opening is a new panel: reopening while the last one still slides away must not bring its old fields back.
  const openings = useRef(0);
  const wasOpen = useRef(false);
  if (open && !wasOpen.current) openings.current += 1;
  wasOpen.current = open;
  const [confirming, setConfirming] = useState<(() => void) | null>(null);
  const confirmRef = useRef<HTMLButtonElement>(null);

  // Each time it opens it starts clean; and what is shown after a save (a link to read) holds nothing to lose.
  useEffect(() => {
    if (open) {
      dirty.current = false;
      setConfirming(null);
    }
  }, [open]);
  useEffect(() => {
    if (explicitClose) {
      dirty.current = false;
      setConfirming(null);
    }
  }, [explicitClose]);
  // The question takes the focus, and gives it back to what had it when the person goes on editing.
  const beforeConfirm = useRef<HTMLElement | null>(null);
  useEffect(() => {
    if (confirming) {
      beforeConfirm.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      confirmRef.current?.focus();
    }
  }, [confirming]);
  function keepEditing() {
    setConfirming(null);
    // After the question has left the page.
    requestAnimationFrame(() => beforeConfirm.current?.focus());
  }

  const guard = useCallback(
    (action: () => void) => {
      if (locked) return;
      if (dirty.current) setConfirming(() => action);
      else action();
    },
    [locked],
  );
  const markDirty = useCallback(() => {
    dirty.current = true;
  }, []);
  const guardValue = useMemo(() => ({ guard, markDirty }), [guard, markDirty]);

  useEffect(() => {
    if (!open) return;
    opener.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    // The page behind does not scroll under a wheel or a finger on the shade.
    const overflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = overflow;
      opener.current?.focus();
    };
  }, [open]);

  // The first field takes the focus, also when the form arrives after the sheet (it waits for what it
  // points at): it only moves the focus while the person has not put it on something inside.
  useEffect(() => {
    const panel = panelRef.current;
    if (!open || !panel) return;
    const inside = document.activeElement !== panel && panel.contains(document.activeElement);
    if (inside) return;
    const field = panel.querySelector(".sheet-body")?.querySelector<HTMLElement>(FOCUSABLE);
    (field ?? panel).focus();
  });

  function keep(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.stopPropagation();
      if (confirming) keepEditing();
      else if (!explicitClose) guard(onClose);
      return;
    }
    if (event.key !== "Tab") return;
    // While the question is up, the Tab key goes round inside it and nowhere else.
    const scope = confirming ? panelRef.current?.querySelector(".sheet-confirm") : panelRef.current;
    const focusable = Array.from(scope?.querySelectorAll<HTMLElement>(FOCUSABLE) ?? []);
    const [first, last] = [focusable[0], focusable[focusable.length - 1]];
    const onPanel = document.activeElement === panelRef.current;
    if (event.shiftKey && (document.activeElement === first || onPanel)) {
      event.preventDefault();
      last?.focus();
    } else if (!event.shiftKey && (document.activeElement === last || onPanel)) {
      event.preventDefault();
      first?.focus();
    }
  }

  // In the body, not in the page: the page enters with a transform, which would make it the box the
  // sheet is fixed to instead of the screen.
  return createPortal(
    <AnimatePresence>
      {open && (
        <div className="sheet-root" onKeyDown={keep}>
          <motion.div
            className="sheet-scrim"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.2 }}
            onClick={explicitClose ? undefined : () => guard(onClose)}
          />
          <motion.aside
            key={openings.current}
            ref={panelRef}
            className="sheet"
            role="dialog"
            aria-modal="true"
            aria-labelledby={titleId}
            tabIndex={-1}
            initial={{ y: "100%" }}
            animate={{ y: 0 }}
            exit={{ y: "100%" }}
            transition={{ type: "spring", stiffness: 380, damping: 40 }}
          >
            <header className="sheet-head" inert={confirming !== null || undefined}>
              <h2 id={titleId}>{title}</h2>
              <Button type="button" variant="ghost" icon={XIcon} onClick={() => guard(onClose)}>
                {t("Close")}
              </Button>
            </header>
            <SheetGuard.Provider value={guardValue}>
              <div className="sheet-body" inert={confirming !== null || undefined} onInput={markDirty} onChange={markDirty}>
                {children}
              </div>
            </SheetGuard.Provider>
            {confirming && (
              <div className="sheet-confirm" role="alertdialog" aria-modal="true" aria-labelledby={`${titleId}-confirm`} aria-describedby={`${titleId}-confirm-text`}>
                <div className="sheet-confirm-box">
                  <h3 id={`${titleId}-confirm`}>{t("Discard edits?")}</h3>
                  <p id={`${titleId}-confirm-text`} className="muted">{t("What you filled in has not been saved yet.")}</p>
                  <div className="form-actions">
                    <Button ref={confirmRef} type="button" variant="ghost" onClick={keepEditing}>
                      {t("Go back and edit")}
                    </Button>
                    <Button
                      type="button"
                      onClick={() => {
                        const action = confirming;
                        setConfirming(null);
                        action();
                      }}
                    >
                      {t("Discard and close")}
                    </Button>
                  </div>
                </div>
              </div>
            )}
          </motion.aside>
        </div>
      )}
    </AnimatePresence>,
    document.body,
  );
}
