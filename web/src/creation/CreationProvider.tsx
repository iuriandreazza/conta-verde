import { AnimatePresence, motion } from "motion/react";
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { BadgeAlertIcon } from "../components/icons/badge-alert";
import { CheckIcon } from "../components/icons/check";
import { Sheet } from "../components/Sheet";
import { EntryCreator } from "./EntryCreator";
import { CostCenterCreator, PaymentMethodCreator, PaymentTermCreator } from "./registers";
import { PartyCreator } from "./PartyCreator";
import { ProductCreator } from "./ProductCreator";
import { TransferCreator } from "./TransferCreator";

export type CreateKind = "entry" | "party" | "product" | "transfer" | "costCenter" | "paymentMethod" | "paymentTerm";

/** What a creator tells the host when the record exists: the words for the confirmation, and the new id. */
export interface Created {
  message: string;
  id: string;
  /** A warning from what was saved (the registry could not confirm a CNPJ) is not a plain success. */
  tone?: "success" | "warning";
}

export interface CreatorProps {
  companyId: string;
  /** Tells the host a save is in flight: the sheet must not close under it. */
  onBusyChange: (busy: boolean) => void;
  onCreated: (created: Created) => void;
  onCancel: () => void;
}

const CREATORS: Record<CreateKind, { title: string; Creator: (props: CreatorProps) => ReactNode }> = {
  entry: { title: "Novo lançamento", Creator: EntryCreator },
  costCenter: { title: "Novo centro de custo", Creator: CostCenterCreator },
  paymentMethod: { title: "Nova forma de pagamento", Creator: PaymentMethodCreator },
  paymentTerm: { title: "Novo prazo de pagamento", Creator: PaymentTermCreator },
  party: { title: "Novo cliente ou fornecedor", Creator: PartyCreator },
  product: { title: "Novo produto ou serviço", Creator: ProductCreator },
  transfer: { title: "Nova transferência", Creator: TransferCreator },
};

interface Creation {
  /** Opens the sheet for a new record over the page that is showing, without leaving it. */
  openCreate: (kind: CreateKind) => void;
  /** Tells the page that a record of this kind was created, so it can read its list again. */
  subscribe: (kind: CreateKind, listener: (created: Created) => void) => () => void;
}

const CreationContext = createContext<Creation | null>(null);

const CONFIRMATION_MS = 6000;

/**
 * "Novo registro" from anywhere: one sheet that rises over whatever page is open, holds the form of
 * the register asked for, and, once it is saved, says so and lets the page read its list again.
 */
export function CreationProvider({ companyId, children }: { companyId: string | undefined; children: ReactNode }) {
  const [kind, setKind] = useState<CreateKind | null>(null);
  // An object, so the same words twice in a row are a new confirmation with a new timer.
  const [confirmation, setConfirmation] = useState<{ text: string; id: number; tone: "success" | "warning" } | null>(null);
  const busy = useRef(false);
  const confirmations = useRef(0);
  const listeners = useRef(new Map<CreateKind, Set<(created: Created) => void>>());

  const openCreate = useCallback((wanted: CreateKind) => setKind(wanted), []);
  const closeUnlessSaving = useCallback(() => {
    if (!busy.current) setKind(null);
  }, []);

  // Another company, or a path outside any: what was open belongs to the one that was left.
  useEffect(() => {
    busy.current = false;
    setKind(null);
  }, [companyId]);

  const subscribe = useCallback((wanted: CreateKind, listener: (created: Created) => void) => {
    const set = listeners.current.get(wanted) ?? new Set();
    set.add(listener);
    listeners.current.set(wanted, set);
    return () => void set.delete(listener);
  }, []);

  useEffect(() => {
    // A warning asks the person to look at something: it stays until it is dismissed.
    if (confirmation === null || confirmation.tone === "warning") return;
    const timer = setTimeout(() => setConfirmation(null), CONFIRMATION_MS);
    return () => clearTimeout(timer);
  }, [confirmation]);

  const value = useMemo(() => ({ openCreate, subscribe }), [openCreate, subscribe]);
  const current = kind === null ? null : CREATORS[kind];

  return (
    <CreationContext.Provider value={value}>
      {children}
      {companyId !== undefined && (
        <Sheet open={kind !== null} title={current?.title ?? ""} onClose={closeUnlessSaving}>
          {kind !== null && current !== null && (
            <current.Creator
              key={companyId}
              companyId={companyId}
              onBusyChange={(value) => {
                busy.current = value;
              }}
              onCancel={closeUnlessSaving}
              onCreated={(created) => {
                busy.current = false;
                setKind(null);
                setConfirmation({ text: created.message, id: ++confirmations.current, tone: created.tone ?? "success" });
                listeners.current.get(kind)?.forEach((listener) => listener(created));
              }}
            />
          )}
        </Sheet>
      )}
      {/* The live region stays in the page and the words arrive in it: that is what gets announced. */}
      <div className="toast" aria-live="polite" aria-atomic="true">
        <AnimatePresence>
          {confirmation !== null && (
            <motion.div
              key={confirmation.id}
              className={`notice notice-${confirmation.tone}`}
              initial={{ opacity: 0, y: 12 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: 12 }}
            >
              {confirmation.tone === "warning" ? <BadgeAlertIcon size={18} aria-hidden /> : <CheckIcon size={18} aria-hidden />}
              <span>{confirmation.text}</span>
              {confirmation.tone === "warning" && (
                <button type="button" className="toast-dismiss" onClick={() => setConfirmation(null)}>
                  Dispensar
                </button>
              )}
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    </CreationContext.Provider>
  );
}

export function useCreation(): Creation {
  const creation = useContext(CreationContext);
  if (creation === null) throw new Error("useCreation needs the CreationProvider");
  return creation;
}

/** A page's list reads again when a record of its kind is created from anywhere. */
export function useOnCreated(kind: CreateKind, listener: (created: Created) => void) {
  const { subscribe } = useCreation();
  const latest = useRef(listener);
  latest.current = listener;
  useEffect(() => subscribe(kind, (created) => latest.current(created)), [kind, subscribe]);
}
