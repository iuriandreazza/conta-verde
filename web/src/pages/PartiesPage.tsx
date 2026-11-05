import { Fragment, useState, type FormEvent } from "react";
import { useParams } from "react-router-dom";
import { NO_FILTERS, api, errorMessage, type Party, type PartyFilters } from "../api";
import { PageHeader } from "../components/PageHeader";
import { DocumentBadge } from "../components/DocumentBadge";
import { ListRow } from "../components/ListRow";
import { Notice, type NoticeTone } from "../components/Notice";
import { Button } from "../components/Button";
import { ChevronDownIcon } from "../components/icons/chevron-down";
import { HistoryIcon } from "../components/icons/history";
import { PlusIcon } from "../components/icons/plus";
import { RefreshCWIcon } from "../components/icons/refresh-cw";
import { UsersIcon } from "../components/icons/users";
import { PartyHistory } from "../components/PartyHistory";
import { Spinner } from "../components/Spinner";
import { usePagedList } from "../lib/usePagedList";

// Layout remounts the page whenever the path changes, so `companyId` never changes within one
// instance: the old list is dropped and a reply still in flight for another company lands on an
// unmounted component.
export function PartiesPage() {
  const { companyId = "" } = useParams();
  const [creating, setCreating] = useState(false);
  const [highlightedId, setHighlightedId] = useState<string | null>(null);
  const [formError, setFormError] = useState<string | null>(null);
  const [registryNotice, setRegistryNotice] = useState<RegistryNotice | null>(null);
  const [verifyingId, setVerifyingId] = useState<string | null>(null);
  const [historyOpenFor, setHistoryOpenFor] = useState<string | null>(null);
  const [verifyNotice, setVerifyNotice] = useState<RegistryNotice | null>(null);

  // The search box updates at once; the list follows once typing pauses.
  const [filters, setFilters] = useState<PartyFilters>(NO_FILTERS);
  const {
    items: parties,
    setItems: setParties,
    nextCursor,
    loading,
    loadingMore,
    error,
    setError,
    applied,
    reload: loadFirstPage,
    loadMore,
  } = usePagedList<Party, PartyFilters>(
    (applied, after) => api.listParties(companyId, applied, after),
    filters,
  );
  const isFiltered = applied.q !== "" || applied.kind !== "";

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
      <PageHeader title="Clientes e fornecedores" description="Quem compra de você e de quem você compra." />
      <div className="split">
        <section className="card list-card">
        <h2>Cadastrados</h2>
        <div className="filters" role="search">
          <label>
            Buscar
            <input
              type="search"
              value={filters.q}
              placeholder="Nome, documento ou e-mail"
              autoComplete="off"
              maxLength={100}
              onChange={(event) => setFilters({ ...filters, q: event.target.value })}
            />
          </label>
          <label>
            Mostrar
            <select
              value={filters.kind}
              onChange={(event) =>
                setFilters({ ...filters, kind: event.target.value as PartyFilters["kind"] })
              }
            >
              <option value="">Todos</option>
              <option value="customer">Clientes</option>
              <option value="supplier">Fornecedores</option>
            </select>
          </label>
          <label>
            Ordem
            <select
              value={filters.sort}
              onChange={(event) =>
                setFilters({ ...filters, sort: event.target.value as PartyFilters["sort"] })
              }
            >
              <option value="name">Nome (A–Z)</option>
              <option value="-name">Nome (Z–A)</option>
            </select>
          </label>
        </div>
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
        ) : parties.length === 0 && isFiltered ? (
          <div className="empty">
            <UsersIcon size={40} aria-hidden />
            <p role="status">Nenhum cadastro encontrado.</p>
            <Button type="button" variant="ghost" onClick={() => setFilters({ ...NO_FILTERS, sort: filters.sort })}>
              Limpar filtros
            </Button>
          </div>
        ) : parties.length === 0 ? (
          <div className="empty">
            <UsersIcon size={40} aria-hidden />
            <p>Nenhum cadastro ainda.</p>
            <p className="muted">Use o formulário para adicionar o primeiro.</p>
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
                  <th>
                    <span className="visually-hidden">Histórico</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {parties.map((party, index) => (
                  <Fragment key={party.id}>
                  <ListRow index={index} highlighted={party.id === highlightedId}>
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
                          disabled={verifyingId !== null || creating}
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
                    <td>
                      <Button
                        type="button"
                        variant="ghost"
                        className="btn-small"
                        aria-label={`Histórico de ${party.name}`}
                        aria-expanded={historyOpenFor === party.id}
                        aria-controls={`history-${party.id}`}
                        icon={HistoryIcon}
                        onClick={() =>
                          setHistoryOpenFor(historyOpenFor === party.id ? null : party.id)
                        }
                      >
                        Histórico
                      </Button>
                    </td>
                  </ListRow>
                  {historyOpenFor === party.id && (
                    <tr id={`history-${party.id}`}>
                      <td colSpan={5} role="region" aria-label={`Histórico de ${party.name}`}>
                        <PartyHistory
                          // Remounted when the check changes, so an open history follows a re-check.
                          key={JSON.stringify(party.document_check)}
                          companyId={companyId}
                          partyId={party.id}
                        />
                      </td>
                    </tr>
                  )}
                  </Fragment>
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
        <aside className="card form-card">
        <h2>Novo cliente ou fornecedor</h2>
        <form onSubmit={handleCreate}>
          <label>
            Nome
            <input name="name" required maxLength={200} />
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
          <Button type="submit" icon={PlusIcon} pending={creating} disabled={verifyingId !== null}>
            Adicionar
          </Button>
        </form>
      </aside>
      </div>
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
      : {
          tone: "success",
          message: `${registering ? "" : `${party.name}: `}CNPJ verificado: ${check.legal_name}.`,
        };
  }
  return null;
}
