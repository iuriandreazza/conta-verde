import { AnimatePresence, motion } from "motion/react";
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { BadgeAlertIcon } from "../components/icons/badge-alert";
import { Button } from "../components/Button";
import { CheckIcon } from "../components/icons/check";
import { Sheet } from "../components/Sheet";
import { EntryCreator } from "./EntryCreator";
import { CategoryCreator } from "./CategoryCreator";
import { RecurrenceCreator } from "./RecurrenceCreator";
import { InviteCreator } from "./InviteCreator";
import { BankAccountCreator, CostCenterCreator, PaymentMethodCreator, PaymentTermCreator } from "./registers";
import { PartyCreator } from "./PartyCreator";
import { ProductCreator } from "./ProductCreator";
import { TransferCreator } from "./TransferCreator";

export type CreateKind = "entry" | "party" | "product" | "transfer" | "costCenter" | "paymentMethod" | "paymentTerm" | "bankAccount" | "invite" | "recurrence" | "category";

/** What a creator tells the host when the record exists: the words for the confirmation, and the new id. */
export interface Created {
  message: string;
  id: string;
  /** A warning from what was saved (the registry could not confirm a CNPJ) is not a plain success. */
  tone?: "success" | "warning";
  /** Something the person must read before leaving (a link that is shown once): the sheet stays open on it. */
  result?: ReactNode;
}

/** What a page may ask a creator to start with: the parent of a new subcategory. */
export interface CreateOptions {
  parentId?: string;
}

export interface CreatorProps {
  companyId: string;
  options: CreateOptions;
  /** Tells the host a save is in flight: the sheet must not close under it. */
  onBusyChange: (busy: boolean) => void;
  onCreated: (created: Created) => void;
  onCancel: () => void;
}

const CREATORS: Record<CreateKind, { title: string; Creator: (props: CreatorProps) => ReactNode }> = {
  entry: { title: "Novo lançamento", Creator: EntryCreator },
  category: { title: "Nova categoria", Creator: CategoryCreator },
  recurrence: { title: "Nova recorrência", Creator: RecurrenceCreator },
  invite: { title: "Convidar alguém", Creator: InviteCreator },
  bankAccount: { title: "Nova conta bancária", Creator: BankAccountCreator },
  costCenter: { title: "Novo centro de custo", Creator: CostCenterCreator },
  paymentMethod: { title: "Nova forma de pagamento", Creator: PaymentMethodCreator },
  paymentTerm: { title: "Novo prazo de pagamento", Creator: PaymentTermCreator },
  party: { title: "Novo cliente ou fornecedor", Creator: PartyCreator },
  product: { title: "Novo produto ou serviço", Creator: ProductCreator },
  transfer: { title: "Nova transferência", Creator: TransferCreator },
};

interface Creation {
  /** Opens the sheet for a new record over the page that is showing, without leaving it. */
  openCreate: (kind: CreateKind, options?: CreateOptions) => void;
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
  // What a creator asks the person to read before the sheet goes away.
  const [result, setResult] = useState<ReactNode>(null);
  const busy = useRef(false);
  const [saving, setSaving] = useState(false);
  const confirmations = useRef(0);
  const listeners = useRef(new Map<CreateKind, Set<(created: Created) => void>>());

  const [options, setOptions] = useState<CreateOptions>({});
  const openCreate = useCallback((wanted: CreateKind, asked: CreateOptions = {}) => {
    setResult(null);
    setOptions(asked);
    setKind(wanted);
  }, []);
  const closeUnlessSaving = useCallback(() => {
    if (busy.current) return;
    setKind(null);
    setResult(null);
  }, []);

  // Another company, or a path outside any: what was open belongs to the one that was left.
  useEffect(() => {
    busy.current = false;
    setSaving(false);
    setKind(null);
    setResult(null);
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
        <Sheet open={kind !== null} title={current?.title ?? ""} onClose={closeUnlessSaving} explicitClose={result !== null} locked={saving}>
          {kind !== null && current !== null && result !== null && (
            <div className="creation-result">
              {result}
              <div className="form-footer">
                <span />
                <Button type="button" icon={CheckIcon} onClick={closeUnlessSaving}>
                  Concluir
                </Button>
              </div>
            </div>
          )}
          {kind !== null && current !== null && result === null && (
            <current.Creator
              key={companyId}
              companyId={companyId}
              options={options}
              onBusyChange={(value) => {
                busy.current = value;
                setSaving(value);
              }}
              onCancel={closeUnlessSaving}
              onCreated={(created) => {
                busy.current = false;
                setSaving(false);
                // The record exists: a listener that fails must not turn that into a failed save.
                listeners.current.get(kind)?.forEach((listener) => {
                  try {
                    listener(created);
                  } catch (failure) {
                    console.error(failure);
                  }
                });
                if (created.result !== undefined) {
                  // The sheet stays, now showing what must be read; the page behind already has the news.
                  setResult(created.result);
                  return;
                }
                setKind(null);
                setConfirmation({ text: created.message, id: ++confirmations.current, tone: created.tone ?? "success" });
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
