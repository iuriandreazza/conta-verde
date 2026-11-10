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
import { Button } from "./Button";
import { History } from "./History";
import { HelpTitle } from "./HelpTitle";
import { IssueForm } from "./FiscalIssueForm";
import { ServiceIssueForm } from "./FiscalServiceIssueForm";
import { Notice } from "./Notice";

const FILE_LABELS: Record<FiscalFileKind, string> = {
  xml: "Baixar XML",
  pdf: "Baixar DANFE (PDF)",
  cancellation_xml: "Baixar XML do cancelamento",
};

/** The printed form of an NFS-e is the DANFSe. */
const SERVICE_FILE_LABELS: Record<FiscalFileKind, string> = { ...FILE_LABELS, pdf: "Baixar DANFSe (PDF)" };

const FIELD_LABELS: Record<string, string> = {
  kind: "Tipo",
  status: "Situação",
  issuer_cnpj: "CNPJ do emitente",
  environment: "Ambiente",
  recipient: "Destinatário",
  nature: "Natureza",
  total: "Total",
  issued_on: "Emitida em",
  access_key: "Chave de acesso",
  number: "Número",
  series: "Série",
  protocol: "Protocolo",
  dps: "DPS (série/número)",
  nfse_number: "Número da NFS-e",
  nfse_key: "Chave da NFS-e",
  verification_code: "Código de verificação",
  gateway_reference: "Referência do gateway",
  refusal: "Recusa",
  note: "Observação",
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
        <dt>Situação</dt>
        <dd>
          <span className="badge">{STATUS_LABELS[document.status]}</span>
        </dd>
        <dt>Tipo</dt>
        <dd>{KIND_LABELS[document.kind]}</dd>
        {isService ? (
          <>
            <dt>Número da NFS-e</dt>
            <dd>{document.nfse ? document.nfse.number : "—"}</dd>
            {document.dps && (
              <>
                <dt>DPS (série/número)</dt>
                <dd>{numberLabel(document.dps.series, document.dps.number)}</dd>
              </>
            )}
            {document.nfse?.verification_code && (
              <>
                <dt>Código de verificação</dt>
                <dd>{document.nfse.verification_code}</dd>
              </>
            )}
            {document.nfse?.key && (
              <>
                <dt>Chave da NFS-e</dt>
                <dd className="access-key">{document.nfse.key}</dd>
              </>
            )}
          </>
        ) : (
          <>
            <dt>Número / série</dt>
            <dd>{numberLabel(document.series, document.number)}</dd>
          </>
        )}
        <dt>Destinatário</dt>
        <dd>
          {document.recipient_name}
          {document.recipient_document ? ` · ${document.recipient_document}` : ""}
        </dd>
        <dt>Total</dt>
        <dd>{formatMoney(document.total)}</dd>
        <dt>Emitida em</dt>
        <dd>
          {formatDate(document.issued_on)} · {document.environment === "production" ? "Produção" : "Homologação"}
        </dd>
        {document.access_key && (
          <>
            <dt>Chave de acesso</dt>
            <dd className="access-key">{document.access_key}</dd>
          </>
        )}
        {document.protocol && (
          <>
            <dt>Protocolo</dt>
            <dd>{document.protocol}</dd>
          </>
        )}
        <dt>Enviada por</dt>
        <dd>{document.submitted_by}</dd>
      </dl>
      {document.refusal && (
        <Notice message={`Recusada (${document.refusal.code}): ${document.refusal.message}`} tone="warning" />
      )}
      {document.note && <Notice message={document.note} tone="warning" />}
      {document.environment === "homologation" && (
        <Notice message="Documento de homologação: não tem valor fiscal." tone="warning" />
      )}

      <div className="form-actions">
        {document.files.map((kind) => (
          <Button key={kind} type="button" pending={pending === kind} disabled={pending !== null} onClick={() => void download(kind)}>
            {(isService ? SERVICE_FILE_LABELS : FILE_LABELS)[kind]}
          </Button>
        ))}
        {canWrite && isWaiting(document.status) && (
          <Button
            type="button"
            pending={pending === "refresh"}
            disabled={pending !== null}
            onClick={() => void run("refresh", async () => onChanged(await api.refreshFiscalDocument(companyId, document.id)))}
          >
            Atualizar situação
          </Button>
        )}
        {canWrite && document.status === "rejected" && document.kind !== "nfce" && (
          <Button type="button" onClick={() => setResending((open) => !open)}>
            Reenviar
          </Button>
        )}
      </div>

      {document.kind === "nfce" && document.status === "rejected" && (
        <Notice tone="warning" message="O reenvio de uma NFC-e recusada ainda não tem tela aqui: use o Caixa logo depois da recusa, ou a API." />
      )}
      {resending && takesLetters && (
        <IssueForm
          companyId={companyId}
          natures={natures}
          fixedSale={document.sale_id}
          submitLabel="Reenviar a mesma nota"
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
          submitLabel="Reenviar a mesma nota (mesmo número de DPS)"
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
            message="O cancelamento foi enviado e a SEFAZ ainda não respondeu. O sistema pergunta de novo sozinho; a nota continua autorizada até lá."
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
              Desistir do cancelamento
            </Button>
          )}
        </>
      )}

      {canWrite && document.status === "authorized" && (
        <>
          {document.cancellation === null && (
          <form className="record-form" onSubmit={(event) => void cancel(event)}>
            <HelpTitle title="Cancelar a nota" level={3}>
              <p>
                O prazo para cancelar é da SEFAZ, não do sistema: se passou, a recusa vem com o motivo dela. Cancelar a nota
                não cancela a venda, e uma nota de um mês fechado só se cancela depois de reabri-lo.
              </p>
            </HelpTitle>
            <label>
              Motivo (de 15 a 255 caracteres)
              <input required minLength={15} maxLength={255} value={reason} onChange={(event) => setReason(event.target.value)} />
            </label>
            <Button type="submit" pending={pending === "cancel"} disabled={pending !== null}>
              Cancelar nota
            </Button>
          </form>
          )}
          {takesLetters && (
          <form className="record-form" onSubmit={(event) => void correct(event)}>
            <HelpTitle title="Carta de correção" level={3}>
              <p>
                Corrige dados que não mudam valores (endereço, transportadora). A SEFAZ aceita até 20 cartas por nota e não
                aceita a que muda um valor. Uma carta enviada sem resposta não é reenviada pelo sistema.
              </p>
            </HelpTitle>
            <label>
              Texto (de 15 a 1000 caracteres)
              <input required minLength={15} maxLength={1000} value={letter} onChange={(event) => setLetter(event.target.value)} />
            </label>
            <Button type="submit" pending={pending === "letter"} disabled={pending !== null}>
              Enviar carta
            </Button>
          </form>
          )}
        </>
      )}

      {letters && letters.length > 0 && (
        <section aria-label="Cartas de correção">
          <h3>Cartas de correção</h3>
          <ul className="plain-list">
            {letters.map((item) => (
              <li key={item.id}>
                {item.sequence}. {item.text} — {EVENT_STATUS_LABELS[item.status]}
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
                    Baixar XML
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
                    Desistir
                  </Button>
                )}
              </li>
            ))}
          </ul>
        </section>
      )}

      <section aria-label="Histórico">
        <h3>Histórico</h3>
        <History
          load={loadHistory}
          fieldLabel={(field) => FIELD_LABELS[field] ?? field}
          valueLabel={(field, value) => {
            if (value === null) return "—";
            if (field === "status") return STATUS_LABELS[value as keyof typeof STATUS_LABELS] ?? value;
            if (field === "environment") return value === "production" ? "Produção" : "Homologação";
            if (field === "total") return formatMoney(value);
            return value;
          }}
          emptyMessage="Ainda não há registros."
        />
      </section>
    </div>
  );
}
