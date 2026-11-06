import { Fragment, useState, type FormEvent } from "react";
import { useParams } from "react-router-dom";
import {
  NO_PAYMENT_METHOD_FILTERS,
  api,
  errorMessage,
  type PaymentMethod,
  type PaymentMethodFilters,
  type PaymentMethodKind,
} from "../api";
import { Button } from "../components/Button";
import { ListRow } from "../components/ListRow";
import { PageHeader } from "../components/PageHeader";
import { Notice } from "../components/Notice";
import { PaymentMethodHistory } from "../components/PaymentMethodHistory";
import { Spinner } from "../components/Spinner";
import { BriefcaseBusinessIcon } from "../components/icons/briefcase-business";
import { CheckIcon } from "../components/icons/check";
import { ChevronDownIcon } from "../components/icons/chevron-down";
import { HistoryIcon } from "../components/icons/history";
import { PlusIcon } from "../components/icons/plus";
import { PAYMENT_METHOD_KINDS, paymentMethodKindLabel } from "../lib/kinds";
import { usePagedList } from "../lib/usePagedList";
import { useCanWrite } from "../lib/useCanWrite";
import { SquarePenIcon } from "../components/icons/square-pen";
import { StatusBadge } from "../components/StatusBadge";

// Layout remounts the page whenever the path changes, so `companyId` never changes within one
// instance: the old list is dropped and a reply still in flight for another company lands on an
// unmounted component.
export function PaymentMethodsPage() {
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("finance");
  const [filters, setFilters] = useState<PaymentMethodFilters>(NO_PAYMENT_METHOD_FILTERS);
  const [editing, setEditing] = useState<PaymentMethod | null>(null);
  // Bumped after a save so the form starts over, empty or for another method.
  const [formVersion, setFormVersion] = useState(0);
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [highlightedId, setHighlightedId] = useState<string | null>(null);
  const [historyOpenFor, setHistoryOpenFor] = useState<string | null>(null);

  const {
    items: methods,
    setItems,
    nextCursor,
    loading,
    loadingMore,
    error,
    applied,
    reload,
    loadMore,
  } = usePagedList<PaymentMethod, PaymentMethodFilters>(
    (applied, after) => api.listPaymentMethods(companyId, applied, after),
    filters,
  );
  const isFiltered = applied.q !== "" || applied.kind !== "" || applied.active !== "";

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (saving) {
      return;
    }
    const form = new FormData(event.currentTarget);
    const input = {
      kind: String(form.get("kind")) as PaymentMethodKind,
      name: String(form.get("name") ?? ""),
      active: form.get("active") === "on",
    };
    setSaving(true);
    setFormError(null);
    setSaved(null);
    try {
      if (editing === null) {
        const created = await api.createPaymentMethod(companyId, input);
        setHighlightedId(created.id);
        setSaved(`${created.name} cadastrada.`);
        setFormVersion((version) => version + 1);
        await reload();
      } else {
        const updated = await api.updatePaymentMethod(companyId, editing.id, input);
        // In place: the list keeps its page and its filters.
        setItems((current) => current.map((item) => (item.id === updated.id ? updated : item)));
        setEditing(null);
        setSaved(`${updated.name} atualizada.`);
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
      <PageHeader title="Formas de pagamento" description="Como a empresa paga e recebe: Pix, boleto, dinheiro, cartões." />
      <div className={canWrite ? "split" : "split split-single"}>
      <section className="card list-card">
        <h2>Cadastradas</h2>
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
            Mostrar
            <select
              value={filters.kind}
              onChange={(event) =>
                setFilters({ ...filters, kind: event.target.value as PaymentMethodFilters["kind"] })
              }
            >
              <option value="">Todas</option>
              {PAYMENT_METHOD_KINDS.map((option) => (
                <option key={option.key} value={option.key}>
                  {option.label}
                </option>
              ))}
            </select>
          </label>
          <label>
            Situação
            <select
              value={filters.active}
              onChange={(event) =>
                setFilters({ ...filters, active: event.target.value as PaymentMethodFilters["active"] })
              }
            >
              <option value="">Ativas e inativas</option>
              <option value="true">Só ativas</option>
              <option value="false">Só inativas</option>
            </select>
          </label>
          <label>
            Ordem
            <select
              value={filters.sort}
              onChange={(event) =>
                setFilters({ ...filters, sort: event.target.value as PaymentMethodFilters["sort"] })
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
        ) : methods.length === 0 && error ? (
          <Button type="button" variant="ghost" onClick={() => void reload()}>
            Tentar de novo
          </Button>
        ) : methods.length === 0 && isFiltered ? (
          <div className="empty">
            <BriefcaseBusinessIcon size={40} aria-hidden />
            <p role="status">Nenhum cadastro encontrado.</p>
            <Button
              type="button"
              variant="ghost"
              onClick={() => setFilters({ ...NO_PAYMENT_METHOD_FILTERS, sort: filters.sort })}
            >
              Limpar filtros
            </Button>
          </div>
        ) : methods.length === 0 ? (
          <div className="empty">
            <BriefcaseBusinessIcon size={40} aria-hidden />
            <p>Nenhuma forma de pagamento ainda.</p>
            <p className="muted">Pix, boleto, dinheiro, cada cartão que você aceita…</p>
          </div>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Nome</th>
                  <th>Tipo</th>
                  <th>Situação</th>
                  <th>
                    <span className="visually-hidden">Ações</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {methods.map((method, index) => (
                  <Fragment key={method.id}>
                    <ListRow index={index} highlighted={method.id === highlightedId}>
                      <td>{method.name}</td>
                      <td>{paymentMethodKindLabel(method.kind)}</td>
                      <td><StatusBadge active={method.active} feminine /></td>
                      <td className="row-actions">
                        {canWrite && (
                          <Button
                            iconOnly
                            icon={SquarePenIcon}
                            type="button"
                            variant="ghost"
                            aria-label={`Alterar ${method.name}`}
                            disabled={saving}
                            onClick={() => {
                              setEditing(method);
                              setFormError(null);
                              setSaved(null);
                            }}
                          >
                            Alterar
                          </Button>
                        )}
                        <Button
                          iconOnly
                          type="button"
                          variant="ghost"
                          aria-label={`Histórico de ${method.name}`}
                          aria-expanded={historyOpenFor === method.id}
                          aria-controls={`history-${method.id}`}
                          icon={HistoryIcon}
                          onClick={() =>
                            setHistoryOpenFor(historyOpenFor === method.id ? null : method.id)
                          }
                        >
                          Histórico
                        </Button>
                      </td>
                    </ListRow>
                    {historyOpenFor === method.id && (
                      <tr id={`history-${method.id}`}>
                        <td colSpan={4} role="region" aria-label={`Histórico de ${method.name}`}>
                          <PaymentMethodHistory
                            // Remounted when the method changes, so an open history follows an edit.
                            key={JSON.stringify(method)}
                            companyId={companyId}
                            methodId={method.id}
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
          <h2>{editing === null ? "Nova forma de pagamento" : `Alterar ${editing.name}`}</h2>
          <form
            // A new key starts the form over for another method, or an empty one after a save.
            key={`${editing?.id ?? "new"}-${formVersion}`}
            onSubmit={(event) => void handleSubmit(event)}
          >
            <label>
              Tipo
              <select name="kind" defaultValue={editing?.kind ?? "pix"}>
                {PAYMENT_METHOD_KINDS.map((option) => (
                  <option key={option.key} value={option.key}>
                    {option.label}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Nome
              <input
                name="name"
                required
                maxLength={200}
                autoComplete="off"
                placeholder="Como aparece nas vendas, por exemplo Cartão Visa"
                // After a save or when editing, the person is in the form: put the cursor back in it.
                autoFocus={editing !== null || formVersion > 0}
                defaultValue={editing?.name}
              />
            </label>
            <label className="inline">
              <input name="active" type="checkbox" defaultChecked={editing?.active ?? true} /> Ativa
            </label>
            <Notice message={formError} />
            <div className="form-actions">
              <Button type="submit" icon={editing === null ? PlusIcon : CheckIcon} pending={saving}>
                {editing === null ? "Adicionar" : "Salvar alterações"}
              </Button>
              {editing !== null && (
                <Button
                  type="button"
                  variant="ghost"
                  disabled={saving}
                  onClick={() => {
                    setEditing(null);
                    setFormError(null);
                  }}
                >
                  Cancelar
                </Button>
              )}
            </div>
          </form>
          <Notice message={saved} tone="success" />
        </aside>
      )}
      </div>

    </>
  );
}
