import { useCallback, useEffect, useState, type FormEvent } from "react";
import {
  api,
  errorMessage,
  type FiscalDocument,
  type FiscalEvent,
  type FiscalFileKind,
  type OperationNature,
} from "../api";
import { saveFile } from "../lib/csv";
import { formatDate } from "../lib/entryInstallments";
import { formatMoney } from "../lib/decimal";
import { EVENT_STATUS_LABELS, KIND_LABELS, STATUS_LABELS, isWaiting, numberLabel } from "../lib/fiscalDocuments";
import { msg, useT } from "../i18n";
import { Button } from "./Button";
import { History } from "./History";
import { HelpTitle } from "./HelpTitle";
import { IssueForm } from "./FiscalIssueForm";
import { ServiceIssueForm } from "./FiscalServiceIssueForm";
import { Notice } from "./Notice";

const FILE_LABELS: Record<FiscalFileKind, string> = {
  xml: msg("Download XML"),
  pdf: msg("Download DANFE (PDF)"),
  cancellation_xml: msg("Download the cancellation XML"),
};

/** The printed form of an NFS-e is the DANFSe. */
const SERVICE_FILE_LABELS: Record<FiscalFileKind, string> = { ...FILE_LABELS, pdf: msg("Download DANFSe (PDF)") };

const FIELD_LABELS: Record<string, string> = {
  kind: msg("Type"),
  status: msg("Status"),
  issuer_cnpj: msg("Issuer CNPJ"),
  environment: msg("Environment"),
  recipient: msg("Recipient"),
  nature: msg("Nature"),
  total: msg("Total"),
  issued_on: msg("Document issued on"),
  access_key: msg("Access key"),
  number: msg("Number"),
  series: msg("Series"),
  protocol: msg("Protocol"),
  dps: msg("DPS (series/number)"),
  nfse_number: msg("NFS-e number"),
  nfse_key: msg("NFS-e key"),
  verification_code: msg("Verification code"),
  gateway_reference: msg("Gateway reference"),
  refusal: msg("Refusal"),
  note: msg("Note"),
};

interface Props {
  companyId: string;
  document: FiscalDocument;
  natures: OperationNature[];
  canWrite: boolean;
  /** The document as the server has it now, after an action. */
  onChanged: (document: FiscalDocument) => void;
}

/** One document: what the authority said, its files, and what can still be done to it. */
export function FiscalDocumentSheet({ companyId, document, natures, canWrite, onChanged }: Props) {
  const t = useT();
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState<string | null>(null);
  const [reason, setReason] = useState("");
  const [letter, setLetter] = useState("");
  const [letters, setLetters] = useState<FiscalEvent[] | null>(null);
  const [resending, setResending] = useState(false);
  const isService = document.kind === "nfse";
  // Only the NF-e takes a correction letter; the NFC-e and the national NFS-e are cancelled and issued again.
  const takesLetters = document.kind === "nfe";

  const loadLetters = useCallback(async () => {
    try {
      setLetters(await api.listCorrections(companyId, document.id));
    } catch (failure) {
      setError(errorMessage(failure));
    }
  }, [companyId, document.id]);

  // Stable between keystrokes, new when the document moves, so the trail is read again only then.
  const loadHistory = useCallback(
    () => api.fiscalDocumentHistory(companyId, document.id),
    [companyId, document.id, document.status],
  );

  useEffect(() => {
    // The national NFS-e has no correction letter.
    if (document.status === "authorized" && takesLetters) void loadLetters();
  }, [document.status, takesLetters, loadLetters]);

  async function run(name: string, action: () => Promise<unknown>) {
    setPending(name);
    setError(null);
    try {
      await action();
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setPending(null);
    }
  }

  const download = (kind: FiscalFileKind) =>
    run(kind, async () => {
      const saved = await api.fiscalDocumentFile(companyId, document.id, kind);
      saveFile(saved.filename, saved.blob);
    });

  async function cancel(event: FormEvent) {
    event.preventDefault();
    await run("cancel", async () => {
      onChanged(await api.cancelFiscalDocument(companyId, document.id, reason));
      setReason("");
    });
  }

  async function correct(event: FormEvent) {
    event.preventDefault();
    await run("letter", async () => {
      await api.sendCorrection(companyId, document.id, letter);
      setLetter("");
    });
    await loadLetters();
  }

  return (
    <div className="record-detail">
      <Notice message={error} />
      <dl className="summary">
        <dt>{t("Status")}</dt>
        <dd>
          <span className="badge">{t(STATUS_LABELS[document.status])}</span>
        </dd>
        <dt>{t("Type")}</dt>
        <dd>{KIND_LABELS[document.kind]}</dd>
        {isService ? (
          <>
            <dt>{t("NFS-e number")}</dt>
            <dd>{document.nfse ? document.nfse.number : "—"}</dd>
            {document.dps && (
              <>
                <dt>{t("DPS (series/number)")}</dt>
                <dd>{numberLabel(document.dps.series, document.dps.number)}</dd>
              </>
            )}
            {document.nfse?.verification_code && (
              <>
                <dt>{t("Verification code")}</dt>
                <dd>{document.nfse.verification_code}</dd>
              </>
            )}
            {document.nfse?.key && (
              <>
                <dt>{t("NFS-e key")}</dt>
                <dd className="access-key">{document.nfse.key}</dd>
              </>
            )}
          </>
        ) : (
          <>
            <dt>{t("Number / series")}</dt>
            <dd>{numberLabel(document.series, document.number)}</dd>
          </>
        )}
        <dt>{t("Recipient")}</dt>
        <dd>
          {document.recipient_name}
          {document.recipient_document ? ` · ${document.recipient_document}` : ""}
        </dd>
        <dt>{t("Total")}</dt>
        <dd>{formatMoney(document.total)}</dd>
        <dt>{t("Document issued on")}</dt>
        <dd>
          {formatDate(document.issued_on)} · {document.environment === "production" ? t("Production") : t("Test environment")}
        </dd>
        {document.access_key && (
          <>
            <dt>{t("Access key")}</dt>
            <dd className="access-key">{document.access_key}</dd>
          </>
        )}
        {document.protocol && (
          <>
            <dt>{t("Protocol")}</dt>
            <dd>{document.protocol}</dd>
          </>
        )}
        <dt>{t("Sent by")}</dt>
        <dd>{document.submitted_by}</dd>
      </dl>
      {document.refusal && (
        <Notice
          message={t("Refused ({code}): {message}", { code: document.refusal.code, message: document.refusal.message })}
          tone="warning"
        />
      )}
      {document.note && <Notice message={document.note} tone="warning" />}
      {document.environment === "homologation" && (
        <Notice message={t("Test document: it has no fiscal value.")} tone="warning" />
      )}

      <div className="form-actions">
        {document.files.map((kind) => (
          <Button key={kind} type="button" pending={pending === kind} disabled={pending !== null} onClick={() => void download(kind)}>
            {t((isService ? SERVICE_FILE_LABELS : FILE_LABELS)[kind])}
          </Button>
        ))}
        {canWrite && isWaiting(document.status) && (
          <Button
            type="button"
            pending={pending === "refresh"}
            disabled={pending !== null}
            onClick={() => void run("refresh", async () => onChanged(await api.refreshFiscalDocument(companyId, document.id)))}
          >
            {t("Refresh status")}
          </Button>
        )}
        {canWrite && document.status === "rejected" && document.kind !== "nfce" && (
          <Button type="button" onClick={() => setResending((open) => !open)}>
            {t("Resend")}
          </Button>
        )}
      </div>

      {document.kind === "nfce" && document.status === "rejected" && (
        <Notice tone="warning" message={t("Resending a refused NFC-e has no screen here yet: use the Cash register right after the refusal, or the API.")} />
      )}
      {resending && takesLetters && (
        <IssueForm
          companyId={companyId}
          natures={natures}
          fixedSale={document.sale_id}
          submitLabel={t("Resend the same invoice")}
          onSubmit={async (choices) => {
            onChanged(await api.resendFiscalDocument(companyId, document.id, choices));
            setResending(false);
          }}
        />
      )}
      {resending && isService && (
        <ServiceIssueForm
          companyId={companyId}
          natures={natures}
          fixedSale={document.sale_id}
          submitLabel={t("Resend the same invoice (same DPS number)")}
          onSubmit={async (choices) => {
            onChanged(await api.resendServiceDocument(companyId, document.id, choices));
            setResending(false);
          }}
        />
      )}

      {document.status === "authorized" && document.cancellation?.status === "sent" && (
        <>
          <Notice
            tone="warning"
            message={t("The cancellation was sent and SEFAZ has not answered yet. The system asks again on its own; the invoice stays authorized until then.")}
          />
          {canWrite && (
            <Button
              type="button"
              pending={pending === "abandon-cancellation"}
              disabled={pending !== null}
              onClick={() =>
                void run("abandon-cancellation", async () => {
                  await api.abandonFiscalEvent(companyId, document.cancellation!.id);
                  onChanged(await api.getFiscalDocument(companyId, document.id));
                })
              }
            >
              {t("Give up on the cancellation")}
            </Button>
          )}
        </>
      )}

      {canWrite && document.status === "authorized" && (
        <>
          {document.cancellation === null && (
          <form className="record-form" onSubmit={(event) => void cancel(event)}>
            <HelpTitle title={t("Cancel the invoice")} level={3}>
              <p>
                {t(
                  "The deadline to cancel is SEFAZ's, not the system's: if it has passed, the refusal comes with its reason. Cancelling the invoice does not cancel the sale, and an invoice from a closed month can only be cancelled after reopening it.",
                )}
              </p>
            </HelpTitle>
            <label>
              {t("Reason (15 to 255 characters)")}
              <input required minLength={15} maxLength={255} value={reason} onChange={(event) => setReason(event.target.value)} />
            </label>
            <Button type="submit" pending={pending === "cancel"} disabled={pending !== null}>
              {t("Cancel invoice")}
            </Button>
          </form>
          )}
          {takesLetters && (
          <form className="record-form" onSubmit={(event) => void correct(event)}>
            <HelpTitle title={t("Correction letter")} level={3}>
              <p>
                {t(
                  "Corrects data that does not change amounts (address, carrier). SEFAZ accepts up to 20 letters per invoice and does not accept one that changes an amount. A letter sent with no answer is not sent again by the system.",
                )}
              </p>
            </HelpTitle>
            <label>
              {t("Text (15 to 1000 characters)")}
              <input required minLength={15} maxLength={1000} value={letter} onChange={(event) => setLetter(event.target.value)} />
            </label>
            <Button type="submit" pending={pending === "letter"} disabled={pending !== null}>
              {t("Send letter")}
            </Button>
          </form>
          )}
        </>
      )}

      {letters && letters.length > 0 && (
        <section aria-label={t("Correction letters")}>
          <h3>{t("Correction letters")}</h3>
          <ul className="plain-list">
            {letters.map((item) => (
              <li key={item.id}>
                {item.sequence}. {item.text} — {t(EVENT_STATUS_LABELS[item.status])}
                {item.refusal ? ` (${item.refusal.code}: ${item.refusal.message})` : ""}
                {item.has_file && (
                  <Button
                    type="button"
                    disabled={pending !== null}
                    onClick={() =>
                      void run("event", async () => {
                        const saved = await api.fiscalEventFile(companyId, item.id);
                        saveFile(saved.filename, saved.blob);
                      })
                    }
                  >
                    {t("Download XML")}
                  </Button>
                )}
                {canWrite && item.status === "sent" && (
                  <Button
                    type="button"
                    disabled={pending !== null}
                    onClick={() =>
                      void run("abandon", async () => {
                        await api.abandonFiscalEvent(companyId, item.id);
                        await loadLetters();
                      })
                    }
                  >
                    {t("Give up")}
                  </Button>
                )}
              </li>
            ))}
          </ul>
        </section>
      )}

      <section aria-label={t("History")}>
        <h3>{t("History")}</h3>
        <History
          load={loadHistory}
          fieldLabel={(field) => (field in FIELD_LABELS ? t(FIELD_LABELS[field]) : field)}
          valueLabel={(field, value) => {
            if (value === null) return "—";
            if (field === "status") return value in STATUS_LABELS ? t(STATUS_LABELS[value as keyof typeof STATUS_LABELS]) : value;
            if (field === "environment") return value === "production" ? t("Production") : t("Test environment");
            if (field === "total") return formatMoney(value);
            return value;
          }}
          emptyMessage={t("No history yet.")}
        />
      </section>
    </div>
  );
}
