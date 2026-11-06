import { AnimatePresence, motion } from "motion/react";
import { createContext, useCallback, useContext, useEffect, useId, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { Button } from "./Button";
import { XIcon } from "./icons/x";

/** Closing, or going back, asks first when something was typed: the way out of the sheet runs through it. */
const SheetGuard = createContext<(action: () => void) => void>((action) => action());

export function useSheetGuard() {
  return useContext(SheetGuard);
}

interface SheetProps {
  open: boolean;
  title: string;
  onClose: () => void;
  /** Only the close button closes it: for what must be read first, a stray click or key must not. */
  explicitClose?: boolean;
  children: ReactNode;
}

const FOCUSABLE = 'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/**
 * A bottom sheet: it rises over the page that is open, so a record is added or changed without
 * leaving the list. A dialog for assistive technology: the focus goes in, stays in until it closes,
 * Escape and the shade close it, and the focus returns to what opened it.
 */
export function Sheet({ open, title, onClose, explicitClose = false, children }: SheetProps) {
  const titleId = useId();
  const panelRef = useRef<HTMLDivElement>(null);
  const opener = useRef<HTMLElement | null>(null);
  const [dirty, setDirty] = useState(false);
  const [confirming, setConfirming] = useState<(() => void) | null>(null);
  const confirmRef = useRef<HTMLButtonElement>(null);

  // Each time it opens it starts clean; and what is shown after a save (a link to read) holds nothing to lose.
  useEffect(() => {
    if (open) {
      setDirty(false);
      setConfirming(null);
    }
  }, [open]);
  useEffect(() => {
    if (explicitClose) setDirty(false);
  }, [explicitClose]);
  useEffect(() => {
    if (confirming) confirmRef.current?.focus();
  }, [confirming]);

  const guard = useCallback(
    (action: () => void) => {
      if (dirty) setConfirming(() => action);
      else action();
    },
    [dirty],
  );

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
      if (confirming) setConfirming(null);
      else if (!explicitClose) guard(onClose);
      return;
    }
    if (event.key !== "Tab") return;
    const focusable = Array.from(panelRef.current?.querySelectorAll<HTMLElement>(FOCUSABLE) ?? []);
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
            <header className="sheet-head">
              <h2 id={titleId}>{title}</h2>
              <Button type="button" variant="ghost" icon={XIcon} onClick={() => guard(onClose)}>
                Fechar
              </Button>
            </header>
            <SheetGuard.Provider value={guard}>
              <div className="sheet-body" onInput={() => setDirty(true)} onChange={() => setDirty(true)}>
                {children}
              </div>
            </SheetGuard.Provider>
            {confirming && (
              <div className="sheet-confirm" role="alertdialog" aria-modal="true" aria-labelledby={`${titleId}-confirm`}>
                <div className="sheet-confirm-box">
                  <h3 id={`${titleId}-confirm`}>Descartar edições?</h3>
                  <p className="muted">O que você preencheu ainda não foi salvo.</p>
                  <div className="form-actions">
                    <Button ref={confirmRef} type="button" variant="ghost" onClick={() => setConfirming(null)}>
                      Voltar e editar
                    </Button>
                    <Button
                      type="button"
                      onClick={() => {
                        const action = confirming;
                        setConfirming(null);
                        action();
                      }}
                    >
                      Descartar e fechar
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
