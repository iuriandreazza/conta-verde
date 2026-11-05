import { Fragment, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useParams } from "react-router-dom";
import { api, errorMessage, type Category, type NewCategory } from "../api";
import { Button } from "../components/Button";
import { CategoryForm, isTransfer } from "../components/CategoryForm";
import { CategoryHistory } from "../components/CategoryHistory";
import { PageHeader } from "../components/PageHeader";
import { ListRow } from "../components/ListRow";
import { Notice } from "../components/Notice";
import { Spinner } from "../components/Spinner";
import { BriefcaseBusinessIcon } from "../components/icons/briefcase-business";
import { HistoryIcon } from "../components/icons/history";
import { PlusIcon } from "../components/icons/plus";
import { treeRows } from "../lib/categoryTree";
import { cashFlowLabel, dreLabel } from "../lib/groups";
import { useCanWrite } from "../lib/useCanWrite";

// Layout remounts the page whenever the path changes, so `companyId` never changes within one
// instance: the old list is dropped and a reply still in flight for another company lands on an
// unmounted component.
export function CategoriesPage() {
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("finance");
  const [categories, setCategories] = useState<Category[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState<Category | null>(null);
  // The parent chosen with "Subcategoria", for the form to start with.
  const [parentForNew, setParentForNew] = useState<string | null>(null);
  // Bumped after a save so the form starts over, empty or for another category.
  const [formVersion, setFormVersion] = useState(0);
  const [saving, setSaving] = useState(false);
  const [applying, setApplying] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [highlightedId, setHighlightedId] = useState<string | null>(null);
  const [historyOpenFor, setHistoryOpenFor] = useState<string | null>(null);
  const formRef = useRef<HTMLElement>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      setCategories((await api.listCategories(companyId)).items);
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setLoading(false);
    }
  }, [companyId]);

  useEffect(() => {
    void load();
  }, [load]);

  const rows = useMemo(() => treeRows(categories), [categories]);

  async function save(work: () => Promise<string>) {
    setSaving(true);
    setFormError(null);
    setSaved(null);
    try {
      setSaved(await work());
      setEditing(null);
      setParentForNew(null);
      setFormVersion((version) => version + 1);
    } catch (failure) {
      setFormError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  function create(input: NewCategory) {
    void save(async () => {
      const created = await api.createCategory(companyId, input);
      setHighlightedId(created.id);
      await load();
      return `${created.name} cadastrada.`;
    });
  }

  function change(category: Category, input: { name: string; active: boolean }) {
    void save(async () => {
      const updated = await api.changeCategory(companyId, category.id, input);
      // In place: the tree stays as it was, with this one changed.
      setCategories((current) => current.map((item) => (item.id === updated.id ? updated : item)));
      return `${updated.name} atualizada.`;
    });
  }

  async function applyTemplate() {
    setApplying(true);
    setError(null);
    try {
      setCategories((await api.applyDefaultCategories(companyId)).items);
    } catch (failure) {
      setError(errorMessage(failure));
      // Another tab may have applied it already: the button would only ever answer 409 again.
      await load();
    } finally {
      setApplying(false);
    }
  }

  function startEditing(category: Category) {
    setEditing(category);
    setParentForNew(null);
    setFormError(null);
    setSaved(null);
    formRef.current?.scrollIntoView?.({ block: "nearest" });
  }

  function startSubcategory(parent: Category) {
    setEditing(null);
    setParentForNew(parent.id);
    setFormVersion((version) => version + 1);
    setFormError(null);
    setSaved(null);
    formRef.current?.scrollIntoView?.({ block: "nearest" });
  }

  return (
    <>
      <PageHeader title="Categorias" description="O plano que organiza entradas e saídas." />
      <div className={canWrite ? "split" : "split split-single"}>
        <section className="card list-card">
        <h2>Plano de categorias</h2>
        <Notice message={error} />
        {loading ? (
          <p role="status" className="status">
            <Spinner /> Carregando…
          </p>
        ) : categories.length === 0 && error ? (
          <Button type="button" variant="ghost" onClick={() => void load()}>
            Tentar de novo
          </Button>
        ) : categories.length === 0 ? (
          <div className="empty">
            <BriefcaseBusinessIcon size={40} aria-hidden />
            <p>Nenhuma categoria ainda.</p>
            <p className="muted">
              As categorias organizam o que entra e sai do caixa e montam o DRE gerencial. Comece pelo
              plano usual de uma pequena empresa ou crie as suas acima.
            </p>
            {canWrite && (
              <Button type="button" icon={PlusIcon} pending={applying} onClick={() => void applyTemplate()}>
                Usar categorias padrão
              </Button>
            )}
          </div>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Nome</th>
                  <th>DRE</th>
                  <th>Fluxo de caixa</th>
                  <th>Situação</th>
                  <th>
                    <span className="visually-hidden">Ações</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {rows.map(({ category, depth, parent }, index) => (
                  <Fragment key={category.id}>
                    <ListRow index={index} highlighted={category.id === highlightedId}>
                      <td style={{ paddingLeft: `${0.5 + depth * 1.25}rem` }}>
                        {parent && <span className="visually-hidden">Subcategoria de {parent.name}: </span>}
                        {category.name}
                      </td>
                      <td>{dreLabel(category.dre_group)}</td>
                      <td>{cashFlowLabel(category.cash_flow_group)}</td>
                      <td>{category.active ? "Ativa" : "Inativa"}</td>
                      <td>
                        {canWrite && (
                          <Button
                            type="button"
                            variant="ghost"
                            className="btn-small"
                            aria-label={`Alterar ${category.name}`}
                            disabled={saving}
                            onClick={() => startEditing(category)}
                          >
                            Alterar
                          </Button>
                        )}
                        {canWrite && category.active && !isTransfer(category) && category.level < 5 && (
                          <Button
                            type="button"
                            variant="ghost"
                            className="btn-small"
                            aria-label={`Nova subcategoria de ${category.name}`}
                            disabled={saving}
                            onClick={() => startSubcategory(category)}
                          >
                            Subcategoria
                          </Button>
                        )}
                        <Button
                          type="button"
                          variant="ghost"
                          className="btn-small"
                          aria-label={`Histórico de ${category.name}`}
                          aria-expanded={historyOpenFor === category.id}
                          aria-controls={`history-${category.id}`}
                          icon={HistoryIcon}
                          onClick={() =>
                            setHistoryOpenFor(historyOpenFor === category.id ? null : category.id)
                          }
                        >
                          Histórico
                        </Button>
                      </td>
                    </ListRow>
                    {historyOpenFor === category.id && (
                      <tr id={`history-${category.id}`}>
                        <td colSpan={5} role="region" aria-label={`Histórico de ${category.name}`}>
                          <CategoryHistory
                            // Remounted when the category changes, so an open history follows an edit.
                            key={JSON.stringify(category)}
                            companyId={companyId}
                            categoryId={category.id}
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
      </section>
        {canWrite && (
          <aside className="card form-card" ref={formRef}>
          <h2>{editing === null ? "Nova categoria" : `Alterar ${editing.name}`}</h2>
          <CategoryForm
            // A new key starts the form over for another category, or an empty one after a save.
            key={`${editing?.id ?? "new"}-${parentForNew ?? ""}-${formVersion}`}
            categories={categories}
            editing={editing}
            initialParentId={parentForNew}
            // After a save or when editing, the person is in the form: put the cursor back in it.
            focusName={editing !== null || parentForNew !== null || formVersion > 0}
            pending={saving}
            error={formError}
            onCreate={create}
            onChange={(input) => editing && change(editing, input)}
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
