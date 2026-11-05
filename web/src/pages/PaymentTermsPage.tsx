import { Fragment, useState } from "react";
import { useParams } from "react-router-dom";
import {
  NO_PAYMENT_TERM_FILTERS,
  api,
  errorMessage,
  type PaymentTerm,
  type PaymentTermFilters,
  type PaymentTermInput,
} from "../api";
import { Button } from "../components/Button";
import { ListRow } from "../components/ListRow";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { PaymentTermForm } from "../components/PaymentTermForm";
import { PaymentTermHistory } from "../components/PaymentTermHistory";
import { Spinner } from "../components/Spinner";
import { BriefcaseBusinessIcon } from "../components/icons/briefcase-business";
import { ChevronDownIcon } from "../components/icons/chevron-down";
import { HistoryIcon } from "../components/icons/history";
import { describeInstallments } from "../lib/installments";
import { usePagedList } from "../lib/usePagedList";
import { useCanWrite } from "../lib/useCanWrite";

// Layout remounts the page whenever the path changes, so `companyId` never changes within one
// instance: the old list is dropped and a reply still in flight for another company lands on an
// unmounted component.
export function PaymentTermsPage() {
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("finance");
  const [filters, setFilters] = useState<PaymentTermFilters>(NO_PAYMENT_TERM_FILTERS);
  const [editing, setEditing] = useState<PaymentTerm | null>(null);
  // Bumped after a save so the form starts over, empty or for another term.
  const [formVersion, setFormVersion] = useState(0);
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [highlightedId, setHighlightedId] = useState<string | null>(null);
  const [historyOpenFor, setHistoryOpenFor] = useState<string | null>(null);

  const {
    items: terms,
    setItems,
    nextCursor,
    loading,
    loadingMore,
    error,
    applied,
    reload,
    loadMore,
  } = usePagedList<PaymentTerm, PaymentTermFilters>(
    (applied, after) => api.listPaymentTerms(companyId, applied, after),
    filters,
  );
  const isFiltered = applied.q !== "" || applied.active !== "";

  async function handleSubmit(input: PaymentTermInput) {
    setSaving(true);
    setFormError(null);
    setSaved(null);
    try {
      if (editing === null) {
        const created = await api.createPaymentTerm(companyId, input);
        setHighlightedId(created.id);
        setSaved(`${created.name} cadastrado.`);
        setFormVersion((version) => version + 1);
        await reload();
      } else {
        const updated = await api.updatePaymentTerm(companyId, editing.id, input);
        // In place: the list keeps its page and its filters.
        setItems((current) => current.map((item) => (item.id === updated.id ? updated : item)));
        setEditing(null);
        setSaved(`${updated.name} atualizado.`);
        setFormVersion((version) => version + 1);
      }
    } catch (failure) {
      setFormError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  return (
    <>
      <PageHeader
        title="Prazos de pagamento"
        description="Como um valor se divide no tempo: à vista, 30/60/90, entrada e saldo."
      />
      <div className={canWrite ? "split" : "split split-single"}>
        <section className="card list-card">
          <h2>Cadastrados</h2>
          <div className="filters" role="search">
            <label>
              Buscar
              <input
                type="search"
                value={filters.q}
                placeholder="Nome"
                autoComplete="off"
                maxLength={100}
                onChange={(event) => setFilters({ ...filters, q: event.target.value })}
              />
            </label>
            <label>
              Situação
              <select
                value={filters.active}
                onChange={(event) =>
                  setFilters({ ...filters, active: event.target.value as PaymentTermFilters["active"] })
                }
              >
                <option value="">Ativos e inativos</option>
                <option value="true">Só ativos</option>
                <option value="false">Só inativos</option>
              </select>
            </label>
            <label>
              Ordem
              <select
                value={filters.sort}
                onChange={(event) =>
                  setFilters({ ...filters, sort: event.target.value as PaymentTermFilters["sort"] })
                }
              >
                <option value="name">Nome (A–Z)</option>
                <option value="-name">Nome (Z–A)</option>
              </select>
            </label>
          </div>
          <Notice message={error} />
          {loading ? (
            <p role="status" className="status">
              <Spinner /> Carregando…
            </p>
          ) : terms.length === 0 && error ? (
            <Button type="button" variant="ghost" onClick={() => void reload()}>
              Tentar de novo
            </Button>
          ) : terms.length === 0 && isFiltered ? (
            <div className="empty">
              <BriefcaseBusinessIcon size={40} aria-hidden />
              <p role="status">Nenhum cadastro encontrado.</p>
              <Button
                type="button"
                variant="ghost"
                onClick={() => setFilters({ ...NO_PAYMENT_TERM_FILTERS, sort: filters.sort })}
              >
                Limpar filtros
              </Button>
            </div>
          ) : terms.length === 0 ? (
            <div className="empty">
              <BriefcaseBusinessIcon size={40} aria-hidden />
              <p>Nenhum prazo de pagamento ainda.</p>
              <p className="muted">À vista, 30 dias, 30/60/90, 50% de entrada e 50% em 30 dias…</p>
            </div>
          ) : (
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Nome</th>
                    <th>Parcelas</th>
                    <th>Situação</th>
                    <th>
                      <span className="visually-hidden">Ações</span>
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {terms.map((term, index) => (
                    <Fragment key={term.id}>
                      <ListRow index={index} highlighted={term.id === highlightedId}>
                        <td>{term.name}</td>
                        <td>{describeInstallments(term.installments)}</td>
                        <td>{term.active ? "Ativo" : "Inativo"}</td>
                        <td>
                          {canWrite && (
                            <Button
                              type="button"
                              variant="ghost"
                              className="btn-small"
                              aria-label={`Alterar ${term.name}`}
                              disabled={saving}
                              onClick={() => {
                                setEditing(term);
                                setFormError(null);
                                setSaved(null);
                              }}
                            >
                              Alterar
                            </Button>
                          )}
                          <Button
                            type="button"
                            variant="ghost"
                            className="btn-small"
                            aria-label={`Histórico de ${term.name}`}
                            aria-expanded={historyOpenFor === term.id}
                            aria-controls={`history-${term.id}`}
                            icon={HistoryIcon}
                            onClick={() =>
                              setHistoryOpenFor(historyOpenFor === term.id ? null : term.id)
                            }
                          >
                            Histórico
                          </Button>
                        </td>
                      </ListRow>
                      {historyOpenFor === term.id && (
                        <tr id={`history-${term.id}`}>
                          <td colSpan={4} role="region" aria-label={`Histórico de ${term.name}`}>
                            <PaymentTermHistory
                              // Remounted when the term changes, so an open history follows an edit.
                              key={JSON.stringify(term)}
                              companyId={companyId}
                              termId={term.id}
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
        {canWrite && (
          <aside className="card form-card">
            <h2>{editing === null ? "Novo prazo de pagamento" : `Alterar ${editing.name}`}</h2>
            <PaymentTermForm
              // A new key starts the form over for another term, or an empty one after a save.
              key={`${editing?.id ?? "new"}-${formVersion}`}
              term={editing}
              // After a save or when editing, the person is in the form: put the cursor back in it.
              focusName={editing !== null || formVersion > 0}
              pending={saving}
              error={formError}
              onSubmit={(input) => void handleSubmit(input)}
              onCancel={() => {
                setEditing(null);
                setFormError(null);
              }}
            />
            <Notice message={saved} tone="success" />
          </aside>
        )}
      </div>
    </>
  );
}
