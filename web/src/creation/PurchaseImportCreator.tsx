import { useEffect, useState } from "react";
import { api, errorMessage, type InvoiceEntryParams, type InvoicePreview } from "../api";
import { PurchaseImportForm } from "../components/PurchaseImportForm";
import { Spinner } from "../components/Spinner";
import { useSalesTargets } from "../lib/useSalesTargets";
import { useT } from "../i18n";
import type { CreatorProps } from "./CreationProvider";

/** Enters a supplier's NF-e: reads the XML for a preview first, then enters it with what the person chose. */
export function PurchaseImportCreator({ companyId, onBusyChange, onCreated, onCancel }: CreatorProps) {
  const t = useT();
  const targets = useSalesTargets(companyId);
  const [file, setFile] = useState<File | null>(null);
  const [preview, setPreview] = useState<InvoicePreview | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    onBusyChange(pending);
  }, [pending, onBusyChange]);

  async function read(chosen: File) {
    setPending(true);
    setError(null);
    try {
      setPreview(await api.previewPurchaseImport(companyId, chosen));
      setFile(chosen);
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setPending(false);
    }
  }

  async function enter(params: InvoiceEntryParams) {
    if (file === null) return;
    setPending(true);
    setError(null);
    try {
      const entered = await api.enterPurchaseImport(companyId, file, params);
      onCreated({
        id: entered.purchase_id,
        message: entered.already_entered
          ? t("This NF-e had already been entered as purchase no. {number}: nothing was done again.", { number: entered.number })
          : t("Purchase no. {number} created from the NF-e.", { number: entered.number }),
      });
    } catch (failure) {
      setError(errorMessage(failure));
      setPending(false);
    }
  }

  if (!targets.loaded) {
    return targets.error === null ? (
      <p role="status" className="status">
        <Spinner /> {t("Loading…")}
      </p>
    ) : (
      <p role="alert">{targets.error}</p>
    );
  }
  return (
    <PurchaseImportForm
      // A new file is a new form: what was chosen for the last one does not carry over.
      key={preview?.access_key ?? "choose"}
      companyId={companyId}
      targets={targets}
      preview={preview}
      pending={pending}
      error={error}
      onRead={(chosen) => void read(chosen)}
      onEnter={(params) => void enter(params)}
      onChangeFile={() => {
        setPreview(null);
        setFile(null);
        setError(null);
      }}
      onCancel={onCancel}
    />
  );
}
