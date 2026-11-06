import { AnimatePresence, motion } from "motion/react";
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Notice } from "../components/Notice";
import { Sheet } from "../components/Sheet";
import { EntryCreator } from "./EntryCreator";

export type CreateKind = "entry";

/** What a creator tells the host when the record exists: the words for the confirmation, and the new id. */
export interface Created {
  message: string;
  id: string;
}

export interface CreatorProps {
  companyId: string;
  onCreated: (created: Created) => void;
  onCancel: () => void;
}

const CREATORS: Record<CreateKind, { title: string; Creator: (props: CreatorProps) => ReactNode }> = {
  entry: { title: "Novo lançamento", Creator: EntryCreator },
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
  const [confirmation, setConfirmation] = useState<string | null>(null);
  const listeners = useRef(new Map<CreateKind, Set<(created: Created) => void>>());

  const openCreate = useCallback((wanted: CreateKind) => setKind(wanted), []);

  const subscribe = useCallback((wanted: CreateKind, listener: (created: Created) => void) => {
    const set = listeners.current.get(wanted) ?? new Set();
    set.add(listener);
    listeners.current.set(wanted, set);
    return () => void set.delete(listener);
  }, []);

  useEffect(() => {
    if (confirmation === null) return;
    const timer = setTimeout(() => setConfirmation(null), CONFIRMATION_MS);
    return () => clearTimeout(timer);
  }, [confirmation]);

  const value = useMemo(() => ({ openCreate, subscribe }), [openCreate, subscribe]);
  const current = kind === null ? null : CREATORS[kind];

  return (
    <CreationContext.Provider value={value}>
      {children}
      {companyId !== undefined && (
        <Sheet open={kind !== null} title={current?.title ?? ""} onClose={() => setKind(null)}>
          {kind !== null && current !== null && (
            <current.Creator
              companyId={companyId}
              onCancel={() => setKind(null)}
              onCreated={(created) => {
                setKind(null);
                setConfirmation(created.message);
                listeners.current.get(kind)?.forEach((listener) => listener(created));
              }}
            />
          )}
        </Sheet>
      )}
      <div className="toast">
        <AnimatePresence>
          {confirmation !== null && (
            <motion.div initial={{ opacity: 0, y: 12 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: 12 }}>
              <Notice message={confirmation} tone="success" />
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
