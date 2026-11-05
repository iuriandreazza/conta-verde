import { Fragment, useState, type FormEvent } from "react";
import { useParams } from "react-router-dom";
import {
  NO_COST_CENTER_FILTERS,
  api,
  errorMessage,
  type CostCenter,
  type CostCenterFilters,
} from "../api";
import { Button } from "../components/Button";
import { PageHeader } from "../components/PageHeader";
import { CostCenterHistory } from "../components/CostCenterHistory";
import { ListRow } from "../components/ListRow";
import { Notice } from "../components/Notice";
import { Spinner } from "../components/Spinner";
import { BriefcaseBusinessIcon } from "../components/icons/briefcase-business";
import { CheckIcon } from "../components/icons/check";
import { ChevronDownIcon } from "../components/icons/chevron-down";
import { HistoryIcon } from "../components/icons/history";
import { PlusIcon } from "../components/icons/plus";
import { usePagedList } from "../lib/usePagedList";

// Layout remounts the page whenever the path changes, so `companyId` never changes within one
// instance: the old list is dropped and a reply still in flight for another company lands on an
// unmounted component.
export function CostCentersPage() {
  const { companyId = "" } = useParams();
  const [filters, setFilters] = useState<CostCenterFilters>(NO_COST_CENTER_FILTERS);
  const [editing, setEditing] = useState<CostCenter | null>(null);
  // Bumped after a save so the form starts over, empty or for another cost center.
  const [formVersion, setFormVersion] = useState(0);
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [highlightedId, setHighlightedId] = useState<string | null>(null);
  const [historyOpenFor, setHistoryOpenFor] = useState<string | null>(null);

  const {
    items: costCenters,
    setItems,
    nextCursor,
    loading,
    loadingMore,
    error,
    applied,
    reload,
    loadMore,
  } = usePagedList<CostCenter, CostCenterFilters>(
    (applied, after) => api.listCostCenters(companyId, applied, after),
    filters,
  );
  const isFiltered = applied.q !== "" || applied.active !== "";

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (saving) {
      return;
    }
    const form = new FormData(event.currentTarget);
    const input = { name: String(form.get("name") ?? ""), active: form.get("active") === "on" };
    setSaving(true);
    setFormError(null);
    setSaved(null);
    try {
      if (editing === null) {
        const created = await api.createCostCenter(companyId, input);
        setHighlightedId(created.id);
        setSaved(`${created.name} cadastrado.`);
        setFormVersion((version) => version + 1);
        await reload();
      } else {
        const updated = await api.updateCostCenter(companyId, editing.id, input);
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
      <PageHeader title="Centros de custo" description="Onde cada gasto e cada receita são lançados." />
      <div className="split">
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
                setFilters({ ...filters, active: event.target.value as CostCenterFilters["active"] })
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
                setFilters({ ...filters, sort: event.target.value as CostCenterFilters["sort"] })
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
        ) : costCenters.length === 0 && error ? (
          <Button type="button" variant="ghost" onClick={() => void reload()}>
            Tentar de novo
          </Button>
        ) : costCenters.length === 0 && isFiltered ? (
          <div className="empty">
            <BriefcaseBusinessIcon size={40} aria-hidden />
            <p role="status">Nenhum cadastro encontrado.</p>
            <Button
              type="button"
              variant="ghost"
              onClick={() => setFilters({ ...NO_COST_CENTER_FILTERS, sort: filters.sort })}
            >
              Limpar filtros
            </Button>
          </div>
        ) : costCenters.length === 0 ? (
          <div className="empty">
            <BriefcaseBusinessIcon size={40} aria-hidden />
            <p>Nenhum centro de custo ainda.</p>
            <p className="muted">Uma loja, um projeto ou um departamento, por exemplo.</p>
          </div>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Nome</th>
                  <th>Situação</th>
                  <th>
                    <span className="visually-hidden">Ações</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {costCenters.map((costCenter, index) => (
                  <Fragment key={costCenter.id}>
                    <ListRow index={index} highlighted={costCenter.id === highlightedId}>
                      <td>{costCenter.name}</td>
                      <td>{costCenter.active ? "Ativo" : "Inativo"}</td>
                      <td>
                        <Button
                          type="button"
                          variant="ghost"
                          className="btn-small"
                          aria-label={`Alterar ${costCenter.name}`}
                          disabled={saving}
                          onClick={() => {
                            setEditing(costCenter);
                            setFormError(null);
                            setSaved(null);
                          }}
                        >
                          Alterar
                        </Button>
                        <Button
                          type="button"
                          variant="ghost"
                          className="btn-small"
                          aria-label={`Histórico de ${costCenter.name}`}
                          aria-expanded={historyOpenFor === costCenter.id}
                          aria-controls={`history-${costCenter.id}`}
                          icon={HistoryIcon}
                          onClick={() =>
                            setHistoryOpenFor(historyOpenFor === costCenter.id ? null : costCenter.id)
                          }
                        >
                          Histórico
                        </Button>
                      </td>
                    </ListRow>
                    {historyOpenFor === costCenter.id && (
                      <tr id={`history-${costCenter.id}`}>
                        <td colSpan={3} role="region" aria-label={`Histórico de ${costCenter.name}`}>
                          <CostCenterHistory
                            // Remounted when the cost center changes, so an open history follows an edit.
                            key={JSON.stringify(costCenter)}
                            companyId={companyId}
                            costCenterId={costCenter.id}
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
        <h2>{editing === null ? "Novo centro de custo" : `Alterar ${editing.name}`}</h2>
        <form
          // A new key starts the form over for another cost center, or an empty one after a save.
          key={`${editing?.id ?? "new"}-${formVersion}`}
          onSubmit={(event) => void handleSubmit(event)}
        >
          <label>
            Nome
            <input
              name="name"
              required
              maxLength={200}
              autoComplete="off"
              // After a save or when editing, the person is in the form: put the cursor back in it.
              autoFocus={editing !== null || formVersion > 0}
              defaultValue={editing?.name}
            />
          </label>
          <label className="inline">
            <input name="active" type="checkbox" defaultChecked={editing?.active ?? true} /> Ativo
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
      </div>
    </>
  );
}
