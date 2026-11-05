import { motion } from "motion/react";
import { useCallback, useEffect, useState, type FormEvent } from "react";
import { useParams } from "react-router-dom";
import { api, errorMessage, type Party } from "../api";
import { useAuth } from "../auth";
import { DocumentBadge } from "../components/DocumentBadge";
import { Notice, type NoticeTone } from "../components/Notice";
import { Button } from "../components/Button";
import { ChevronDownIcon } from "../components/icons/chevron-down";
import { PlusIcon } from "../components/icons/plus";
import { RefreshCWIcon } from "../components/icons/refresh-cw";
import { UsersIcon } from "../components/icons/users";
import { Spinner } from "../components/Spinner";

// Rows enter one after another, but never wait longer than the eighth for a long page.
const MAX_STAGGERED_ROWS = 8;

// Layout remounts the page whenever the path changes, so `companyId` never changes within one
// instance: the old list is dropped and a reply still in flight for another company lands on an
// unmounted component.
export function PartiesPage() {
  const { companyId = "" } = useParams();
  const { session } = useAuth();
  const [parties, setParties] = useState<Party[]>([]);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [creating, setCreating] = useState(false);
  const [highlightedId, setHighlightedId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [formError, setFormError] = useState<string | null>(null);
  const [registryNotice, setRegistryNotice] = useState<RegistryNotice | null>(null);
  const [verifyingId, setVerifyingId] = useState<string | null>(null);
  const [verifyNotice, setVerifyNotice] = useState<RegistryNotice | null>(null);

  const companyName =
    session.status === "authenticated"
      ? session.profile.companies.find((company) => company.id === companyId)?.name
      : undefined;

  // Only the very first load shows the spinner; refreshing after an insert keeps the rows on screen.
  const loadFirstPage = useCallback(async () => {
    setError(null);
    try {
      const page = await api.listParties(companyId);
      setParties(page.items);
      setNextCursor(page.next_cursor);
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setLoading(false);
    }
  }, [companyId]);

  useEffect(() => {
    void loadFirstPage();
  }, [loadFirstPage]);

  async function loadMore() {
    if (nextCursor === null || loadingMore) {
      return;
    }
    setLoadingMore(true);
    setError(null);
    try {
      const page = await api.listParties(companyId, nextCursor);
      setParties((current) => [...current, ...page.items]);
      setNextCursor(page.next_cursor);
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setLoadingMore(false);
    }
  }

  async function verifyAgain(party: Party) {
    if (verifyingId !== null) {
      return;
    }
    setVerifyingId(party.id);
    setError(null);
    setVerifyNotice(null);
    try {
      const updated = await api.verifyDocument(companyId, party.id);
      setParties((current) => current.map((item) => (item.id === updated.id ? updated : item)));
      setVerifyNotice(registryNoticeFor(updated, "rechecking"));
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setVerifyingId(null);
    }
  }

  async function handleCreate(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (creating) {
      return;
    }
    const formElement = event.currentTarget;
    const form = new FormData(formElement);
    const optional = (name: string) => String(form.get(name) ?? "").trim() || undefined;
    setCreating(true);
    setFormError(null);
    setRegistryNotice(null);
    // A message about an earlier re-check would read like a statement about the new state.
    setVerifyNotice(null);
    try {
      const created = await api.createParty(companyId, {
        name: String(form.get("name") ?? ""),
        document: optional("document"),
        email: optional("email"),
        is_customer: form.get("is_customer") === "on",
        is_supplier: form.get("is_supplier") === "on",
      });
      formElement.reset();
      setHighlightedId(created.id);
      setRegistryNotice(registryNoticeFor(created, "registering"));
      await loadFirstPage();
    } catch (failure) {
      setFormError(errorMessage(failure));
    } finally {
      setCreating(false);
    }
  }

  return (
    <>
      <h1>{companyName ?? "Clientes e fornecedores"}</h1>

      <section className="card">
        <h2>Novo cliente ou fornecedor</h2>
        <form onSubmit={handleCreate}>
          <label>
            Nome
            <input name="name" required />
          </label>
          <label>
            CPF ou CNPJ
            {/* No numeric keypad: a CNPJ may contain letters since July 2026. */}
            <input
              name="document"
              autoCapitalize="characters"
              autoComplete="off"
              spellCheck={false}
            />
          </label>
          <label>
            E-mail
            <input name="email" type="email" />
          </label>
          <fieldset>
            <legend>Tipo</legend>
            <label className="inline">
              <input name="is_customer" type="checkbox" defaultChecked /> Cliente
            </label>
            <label className="inline">
              <input name="is_supplier" type="checkbox" /> Fornecedor
            </label>
          </fieldset>
          <Notice message={formError} />
          <Notice message={registryNotice?.message ?? null} tone={registryNotice?.tone} />
          <Button type="submit" icon={PlusIcon} pending={creating}>
            Adicionar
          </Button>
        </form>
      </section>

      <section className="card">
        <h2>Cadastrados</h2>
        <Notice message={error} />
        <Notice message={verifyNotice?.message ?? null} tone={verifyNotice?.tone} />
        {loading ? (
          <p role="status" className="status">
            <Spinner /> Carregando…
          </p>
        ) : parties.length === 0 && error ? (
          // The list could not be loaded at all: offer a retry instead of a table with no rows.
          <Button type="button" variant="ghost" onClick={() => void loadFirstPage()}>
            Tentar de novo
          </Button>
        ) : parties.length === 0 ? (
          <div className="empty">
            <UsersIcon size={40} aria-hidden />
            <p>Nenhum cadastro ainda.</p>
            <p className="muted">Use o formulário acima para adicionar o primeiro.</p>
          </div>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Nome</th>
                  <th>Documento</th>
                  <th>E-mail</th>
                  <th>Tipo</th>
                </tr>
              </thead>
              <tbody>
                {parties.map((party, index) => (
                  <motion.tr
                    key={party.id}
                    initial={{ opacity: 0, y: 8, backgroundColor: highlightColor(party, highlightedId, 1) }}
                    animate={{
                      opacity: 1,
                      y: 0,
                      backgroundColor: highlightColor(party, highlightedId, 0),
                    }}
                    transition={{
                      delay: Math.min(index, MAX_STAGGERED_ROWS) * 0.03,
                      duration: party.id === highlightedId ? 1.4 : 0.2,
                    }}
                  >
                    <td>{party.name}</td>
                    <td>
                      {party.document ?? "—"} <DocumentBadge check={party.document_check} />
                      {party.document_check.needs_attention && (
                        <Button
                          type="button"
                          variant="ghost"
                          className="btn-small"
                          aria-label={`Verificar de novo o CNPJ de ${party.name}`}
                          icon={RefreshCWIcon}
                          pending={verifyingId === party.id}
                          disabled={verifyingId !== null}
                          onClick={() => void verifyAgain(party)}
                        >
                          Verificar de novo
                        </Button>
                      )}
                    </td>
                    <td>{party.email ?? "—"}</td>
                    <td>
                      {[party.is_customer && "Cliente", party.is_supplier && "Fornecedor"]
                        .filter(Boolean)
                        .join(" · ")}
                    </td>
                  </motion.tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        {nextCursor !== null && (
          <Button
            type="button"
            variant="ghost"
            icon={ChevronDownIcon}
            pending={loadingMore}
            onClick={() => void loadMore()}
          >
            Carregar mais
          </Button>
        )}
      </section>
    </>
  );
}

interface RegistryNotice {
  message: string;
  tone: NoticeTone;
}

// The party is already saved when this runs: the notice only reports what the registry said,
// so a warning never asks the user to redo anything. `after` says which action produced the
// answer, since "Cadastrado" would be wrong for a re-check of an old row.
function registryNoticeFor(party: Party, after: "registering" | "rechecking"): RegistryNotice | null {
  const check = party.document_check;
  const registering = after === "registering";
  const lead = registering ? "Cadastrado, mas" : `${party.name}:`;
  if (check.status === "not_found") {
    return {
      tone: "warning",
      message: `${lead} a Receita não encontrou este CNPJ. Confira o número: ela pode demorar a incluir empresas recém-abertas.`,
    };
  }
  if (check.status === "unverified") {
    return {
      tone: "warning",
      message: registering
        ? "Cadastrado, mas não foi possível consultar o CNPJ na Receita agora. Só os dígitos foram conferidos."
        : `${party.name}: a Receita continua indisponível. Tente de novo mais tarde.`,
    };
  }
  if (check.status === "verified") {
    return check.needs_attention
      ? {
          tone: "warning",
          message: `${registering ? "Cadastrado. Atenção:" : `${party.name}:`} ${check.legal_name} está com situação ${check.situation} na Receita.`,
        }
      : { tone: "success", message: `CNPJ verificado: ${check.legal_name}.` };
  }
  return null;
}

// A freshly added row flashes green and fades back to transparent.
function highlightColor(party: Party, highlightedId: string | null, alpha: 0 | 1): string {
  return `rgba(47, 178, 110, ${party.id === highlightedId ? alpha * 0.25 : 0})`;
}
