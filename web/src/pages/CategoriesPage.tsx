import { Fragment, useCallback, useEffect, useMemo, useState } from "react";
import { useParams } from "react-router-dom";
import { api, errorMessage, type Category } from "../api";
import { Button } from "../components/Button";
import { CategoryForm, isTransfer } from "../components/CategoryForm";
import { CategoryHistory } from "../components/CategoryHistory";
import { Sheet } from "../components/Sheet";
import { useCreation, useOnCreated } from "../creation/CreationProvider";
import { useLastValue } from "../lib/useLastValue";
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
import { useT } from "../i18n";
import { SquarePenIcon } from "../components/icons/square-pen";
import { StatusBadge } from "../components/StatusBadge";

// Layout remounts the page whenever the path changes, so `companyId` never changes within one
// instance: the old list is dropped and a reply still in flight for another company lands on an
// unmounted component.
export function CategoriesPage() {
  const t = useT();
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("finance");
  const [categories, setCategories] = useState<Category[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState<Category | null>(null);
  const [saving, setSaving] = useState(false);
  const [applying, setApplying] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [highlightedId, setHighlightedId] = useState<string | null>(null);
  const [historyOpenFor, setHistoryOpenFor] = useState<string | null>(null);

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

  const { openCreate } = useCreation();
  const shown = useLastValue(editing);
  // A category made from the top bar or from here: read the plan again and flash the new row.
  useOnCreated("category", (created) => {
    setSaved(null);
    setHighlightedId(created.id);
    void load();
  });

  function closeSheet() {
    // A change on its way finishes first: closing now would hide what a failure has to say.
    if (saving) return;
    setEditing(null);
    setFormError(null);
  }

  async function change(category: Category, input: { name: string; active: boolean }) {
    setSaving(true);
    setFormError(null);
    setSaved(null);
    try {
      const updated = await api.changeCategory(companyId, category.id, input);
      // In place: the tree stays as it was, with this one changed.
      setCategories((current) => current.map((item) => (item.id === updated.id ? updated : item)));
      setEditing(null);
      setSaved(t("Category {name} updated.", { name: updated.name }));
    } catch (failure) {
      setFormError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
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
    setFormError(null);
    setSaved(null);
  }

  return (
    <>
      <PageHeader title={t("Categories")} description={t("The plan that organizes what comes in and what goes out.")} />
      {canWrite && (
        <div className="list-actions">
          <Button type="button" icon={PlusIcon} onClick={() => openCreate("category")}>
            {t("New category")}
          </Button>
        </div>
      )}
      <Notice message={saved} tone="success" />
      <div>
        <section className="card list-card">
        <h2>{t("Category plan")}</h2>
        <Notice message={error} />
        {loading ? (
          <p role="status" className="status">
            <Spinner /> {t("Loading…")}
          </p>
        ) : categories.length === 0 && error ? (
          <Button type="button" variant="ghost" onClick={() => void load()}>
            {t("Try again")}
          </Button>
        ) : categories.length === 0 ? (
          <div className="empty">
            <BriefcaseBusinessIcon size={40} aria-hidden />
            <p>{t("No categories yet.")}</p>
            <p className="muted">
              {t("Categories organize what comes in and goes out of the cash and build the management income statement (DRE).")}{" "}
              {canWrite
                ? t("Start from the usual plan of a small business or create your own with the button above.")
                : t("Whoever handles the company's finances can create the plan.")}
            </p>
            {canWrite && (
              <Button type="button" icon={PlusIcon} pending={applying} onClick={() => void applyTemplate()}>
                {t("Use default categories")}
              </Button>
            )}
          </div>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>{t("Name")}</th>
                  <th>{t("DRE")}</th>
                  <th>{t("Cash flow")}</th>
                  <th>{t("Status")}</th>
                  <th>
                    <span className="visually-hidden">{t("Actions")}</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {rows.map(({ category, depth, parent }, index) => (
                  <Fragment key={category.id}>
                    <ListRow index={index} highlighted={category.id === highlightedId}>
                      <td style={{ paddingLeft: `${0.5 + depth * 1.25}rem` }}>
                        {parent && <span className="visually-hidden">{t("Subcategory of {name}:", { name: parent.name })} </span>}
                        {category.name}
                      </td>
                      <td>{dreLabel(category.dre_group, t)}</td>
                      <td>{cashFlowLabel(category.cash_flow_group, t)}</td>
                      <td><StatusBadge active={category.active} feminine /></td>
                      <td className="row-actions">
                        {canWrite && (
                          <Button
                            iconOnly
                            icon={SquarePenIcon}
                            type="button"
                            variant="ghost"
                            aria-label={t("Edit {name}", { name: category.name })}
                            disabled={saving}
                            onClick={() => startEditing(category)}
                          >
                            {t("Change")}
                          </Button>
                        )}
                        {canWrite && category.active && !isTransfer(category) && category.level < 5 && (
                          <Button
                            iconOnly
                            icon={PlusIcon}
                            type="button"
                            variant="ghost"
                            aria-label={t("New subcategory of {name}", { name: category.name })}
                            disabled={saving}
                            onClick={() => openCreate("category", { parentId: category.id })}
                          >
                            {t("Subcategory")}
                          </Button>
                        )}
                        <Button
                          iconOnly
                          type="button"
                          variant="ghost"
                          aria-label={t("History of {name}", { name: category.name })}
                          aria-expanded={historyOpenFor === category.id}
                          aria-controls={`history-${category.id}`}
                          icon={HistoryIcon}
                          onClick={() =>
                            setHistoryOpenFor(historyOpenFor === category.id ? null : category.id)
                          }
                        >
                          {t("History")}
                        </Button>
                      </td>
                    </ListRow>
                    {historyOpenFor === category.id && (
                      <tr id={`history-${category.id}`}>
                        <td colSpan={5} role="region" aria-label={t("History of {name}", { name: category.name })}>
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
          <Sheet open={editing !== null} locked={saving} title={t("Edit {name}", { name: shown?.name ?? "" })} onClose={closeSheet}>
            {shown !== null && (
              <CategoryForm
                key={shown.id}
                categories={categories}
                editing={shown}
                initialParentId={null}
                pending={saving}
                error={formError}
                onCreate={() => undefined}
                onChange={(input) => void change(shown, input)}
                onCancel={closeSheet}
              />
            )}
          </Sheet>
        )}
      </div>
    </>
  );
}
