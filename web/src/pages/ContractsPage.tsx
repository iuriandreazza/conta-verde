import { Fragment, useCallback, useState } from "react";
import { useParams } from "react-router-dom";
import {
  NO_CONTRACT_FILTERS,
  api,
  errorMessage,
  type BillingCycle,
  type Contract,
  type ContractFilters,
  type ContractInput,
  type ContractStatus,
} from "../api";
import { Button } from "../components/Button";
import { History } from "../components/History";
import { CheckIcon } from "../components/icons/check";
import { ChevronDownIcon } from "../components/icons/chevron-down";
import { HistoryIcon } from "../components/icons/history";
import { PlusIcon } from "../components/icons/plus";
import { RotateCCWIcon } from "../components/icons/rotate-ccw";
import { SquarePenIcon } from "../components/icons/square-pen";
import { ListRow } from "../components/ListRow";
import { Notice } from "../components/Notice";
import { ContractForm } from "../components/ContractForm";
import { Sheet } from "../components/Sheet";
import { useCreation, useOnCreated } from "../creation/CreationProvider";
import { useContractTargets } from "../lib/useContractTargets";
import { useLastValue } from "../lib/useLastValue";
import { PageHeader } from "../components/PageHeader";
import { Spinner } from "../components/Spinner";
import { CONTRACT_STATUS_LABELS, CYCLE_LABELS } from "../lib/contractCycle";
import { formatMoney, isAmbiguousDecimal, toApiDecimal } from "../lib/decimal";
import { formatDate, isIsoDate } from "../lib/entryInstallments";
import { todayIso } from "../lib/settlement";
import { useCanWrite } from "../lib/useCanWrite";
import { usePagedList } from "../lib/usePagedList";

const FIELD_LABELS: Record<string, string> = {
  name: "Nome",
  customer_id: "Cliente",
  seller_id: "Vendedor",
  payment_term_id: "Prazo de pagamento",
  category_id: "Categoria",
  cost_center_id: "Centro de custo",
  notes: "Observações",
  lines: "Itens",
  cycle: "Ciclo",
  starts_on: "Primeira cobrança",
  ends_on: "Última cobrança",
  status: "Situação",
};

type Panel = { kind: "adjust" | "terminate" | "history"; id: string };

// Layout remounts the page whenever the path changes, so `companyId` never changes within one instance.
export function ContractsPage() {
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("sales");
  const [filters, setFilters] = useState<ContractFilters>(NO_CONTRACT_FILTERS);
  const [editing, setEditing] = useState<Contract | null>(null);
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [highlightedId, setHighlightedId] = useState<string | null>(null);
  const [panel, setPanel] = useState<Panel | null>(null);
  const [acting, setActing] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const targets = useContractTargets(companyId);
  const { names, error: targetsError, loaded: targetsLoaded } = targets;

  const { items, setItems, nextCursor, loading, loadingMore, error, applied, reload, loadMore } =
    usePagedList<Contract, ContractFilters>((applied, after) => api.listContracts(companyId, applied, after), filters);
  const isFiltered = applied.q !== "" || applied.status !== "";

  const { openCreate } = useCreation();
  const shown = useLastValue(editing);
  // A contract made from the top bar or from here: read the list again and flash the new row.
  useOnCreated("contract", (created) => {
    setSaved(null);
    setHighlightedId(created.id);
    void reload();
  });

  function closeSheet() {
    // A change on its way finishes first: closing now would hide what a failure has to say.
    if (saving) return;
    setEditing(null);
    setFormError(null);
  }

  async function handleSubmit(input: ContractInput) {
    if (editing === null) return;
    setSaving(true);
    setFormError(null);
    setSaved(null);
    try {
      const updated = await api.updateContract(companyId, editing.id, input);
      setItems((current) => current.map((item) => (item.id === updated.id ? updated : item)));
      setEditing(null);
      setSaved(`${updated.name} atualizado. O que já foi cobrado não muda.`);
    } catch (failure) {
      setFormError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  async function act(run: () => Promise<Contract>, message: (done: Contract) => string) {
    if (acting) {
      return;
    }
    setActing(true);
    setActionError(null);
    setSaved(null);
    try {
      const updated = await run();
      setItems((current) => current.map((item) => (item.id === updated.id ? updated : item)));
      // A form open for this contract would save its old prices or its old status over what was just done.
      if (editing?.id === updated.id) {
        setEditing(null);
      }
      setPanel(null);
      setSaved(message(updated));
    } catch (failure) {
      setActionError(errorMessage(failure));
    } finally {
      setActing(false);
    }
  }

  const historyLoader = useCallback((id: string) => () => api.contractHistory(companyId, id), [companyId]);
  const toggle = (kind: Panel["kind"], id: string) => {
    setPanel(panel?.kind === kind && panel.id === id ? null : { kind, id });
    setActionError(null);
  };

  return (
    <>
      <PageHeader
        title="Contratos"
        description="Cobranças que se repetem: cada ciclo vira uma venda concluída e a conta a receber, sozinhos, no dia."
      />
      {canWrite && (
        <div className="list-actions">
          <Button type="button" icon={PlusIcon} onClick={() => openCreate("contract")}>
            Novo contrato
          </Button>
        </div>
      )}
      <Notice message={saved} tone="success" />
      <div>
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
                value={filters.status}
                onChange={(event) => setFilters({ ...filters, status: event.target.value as ContractFilters["status"] })}
              >
                <option value="">Todas</option>
                {Object.entries(CONTRACT_STATUS_LABELS).map(([key, label]) => (
                  <option key={key} value={key}>
                    {label}
                  </option>
                ))}
              </select>
            </label>
          </div>
          <Notice message={error ?? targetsError} />
          {loading ? (
            <p role="status" className="status">
              <Spinner /> Carregando…
            </p>
          ) : items.length === 0 && error ? (
            <Button type="button" variant="ghost" onClick={() => void reload()}>
              Tentar de novo
            </Button>
          ) : items.length === 0 && isFiltered ? (
            <div className="empty">
              <RotateCCWIcon size={40} aria-hidden />
              <p role="status">Nenhum contrato encontrado.</p>
              <Button type="button" variant="ghost" onClick={() => setFilters({ ...NO_CONTRACT_FILTERS })}>
                Limpar filtros
              </Button>
            </div>
          ) : items.length === 0 ? (
            <div className="empty">
              <RotateCCWIcon size={40} aria-hidden />
              <p>Nenhum contrato ainda.</p>
              <p className="muted">Uma mensalidade, uma manutenção, uma assinatura: descreva uma vez e ele se cobra.</p>
            </div>
          ) : (
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Contrato</th>
                    <th>Ciclo</th>
                    <th className="num">Cada ciclo</th>
                    <th>Situação</th>
                    <th>
                      <span className="visually-hidden">Ações</span>
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {items.map((contract, index) => (
                    <Fragment key={contract.id}>
                      <ListRow index={index} highlighted={contract.id === highlightedId}>
                        <td>
                          <div className="cell-title">{contract.name}</div>
                          <div className="muted">{names.get(contract.customer_id) ?? ""}</div>
                        </td>
                        <td>
                          {CYCLE_LABELS[contract.cycle]}
                          <div className="muted">
                            desde {formatDate(contract.starts_on)}
                            {contract.ends_on !== null && ` até ${formatDate(contract.ends_on)}`}
                          </div>
                        </td>
                        <td className="num">{formatMoney(contract.total)}</td>
                        <td>
                          <span className={statusClass(contract.status)}>{CONTRACT_STATUS_LABELS[contract.status]}</span>
                          {contract.billed_through !== null && (
                            <div className="muted">cobrado até {formatDate(contract.billed_through)}</div>
                          )}
                        </td>
                        <td className="row-actions">
                          {canWrite && contract.status !== "terminated" && (
                            <>
                              <Button
                                iconOnly
                                icon={SquarePenIcon}
                                type="button"
                                variant="ghost"
                                aria-label={`Alterar ${contract.name}`}
                                disabled={saving}
                                onClick={() => {
                                  setEditing(contract);
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
                                aria-label={`Reajustar ${contract.name}`}
                                aria-expanded={panel?.kind === "adjust" && panel.id === contract.id}
                                onClick={() => toggle("adjust", contract.id)}
                              >
                                Reajustar
                              </Button>
                              <Button
                                type="button"
                                variant="ghost"
                                className="btn-small"
                                aria-label={`Encerrar ${contract.name}`}
                                aria-expanded={panel?.kind === "terminate" && panel.id === contract.id}
                                onClick={() => toggle("terminate", contract.id)}
                              >
                                Encerrar
                              </Button>
                            </>
                          )}
                          <Button
                            iconOnly
                            type="button"
                            variant="ghost"
                            aria-label={`Histórico de ${contract.name}`}
                            aria-expanded={panel?.kind === "history" && panel.id === contract.id}
                            icon={HistoryIcon}
                            onClick={() => toggle("history", contract.id)}
                          >
                            Histórico
                          </Button>
                        </td>
                      </ListRow>
                      {panel?.id === contract.id && panel.kind === "adjust" && (
                        <tr>
                          <td colSpan={5} role="region" aria-label={`Reajuste de ${contract.name}`}>
                            <form
                              className="opening-form"
                              onSubmit={(event) => {
                                event.preventDefault();
                                const typed = String(new FormData(event.currentTarget).get("percent") ?? "");
                                if (isAmbiguousDecimal(typed) || toApiDecimal(typed) === undefined) {
                                  setActionError("Informe a porcentagem, por exemplo 5,5 ou -3.");
                                  return;
                                }
                                void act(
                                  () => api.adjustContract(companyId, contract.id, toApiDecimal(typed) ?? ""),
                                  (done) => `${done.name} reajustado: cada ciclo agora cobra ${formatMoney(done.total)}, a partir do próximo.`,
                                );
                              }}
                            >
                              <label>
                                Reajuste (%)
                                <input name="percent" inputMode="decimal" autoComplete="off" placeholder="5,5" required />
                              </label>
                              <Button type="submit" icon={CheckIcon} pending={acting}>
                                Aplicar reajuste
                              </Button>
                              <p className="muted">
                                Muda o preço de cada item (arredondado ao centavo) a partir do próximo ciclo; o que já foi
                                cobrado não muda.
                              </p>
                              <Notice message={actionError} />
                            </form>
                          </td>
                        </tr>
                      )}
                      {panel?.id === contract.id && panel.kind === "terminate" && (
                        <tr>
                          <td colSpan={5} role="region" aria-label={`Encerramento de ${contract.name}`}>
                            <form
                              className="opening-form"
                              onSubmit={(event) => {
                                event.preventDefault();
                                const on = String(new FormData(event.currentTarget).get("on") ?? "");
                                if (!isIsoDate(on)) {
                                  setActionError("Informe o último dia de cobrança.");
                                  return;
                                }
                                void act(
                                  () => api.terminateContract(companyId, contract.id, on),
                                  (done) => `${done.name} encerrado: nada será cobrado depois de ${formatDate(on)}.`,
                                );
                              }}
                            >
                              <label>
                                Último dia de cobrança
                                <input name="on" type="date" required defaultValue={todayIso()} />
                              </label>
                              <Button type="submit" pending={acting}>
                                Encerrar contrato
                              </Button>
                              <p className="muted">Encerrar não se desfaz: para voltar a cobrar, cadastre outro contrato.</p>
                              <Notice message={actionError} />
                            </form>
                          </td>
                        </tr>
                      )}
                      {panel?.id === contract.id && panel.kind === "history" && (
                        <tr>
                          <td colSpan={5} role="region" aria-label={`Histórico de ${contract.name}`}>
                            <History
                              load={historyLoader(contract.id)}
                              fieldLabel={(field) => FIELD_LABELS[field] ?? field}
                              valueLabel={(field, value) => displayValue(field, value, names)}
                              emptyMessage="Sem registros."
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
            <Button type="button" variant="ghost" icon={ChevronDownIcon} pending={loadingMore} onClick={() => void loadMore()}>
              Carregar mais
            </Button>
          )}
        </section>
        {canWrite && (
          <Sheet open={editing !== null} locked={saving} title={`Alterar ${shown?.name ?? ""}`} onClose={closeSheet}>
            {shown !== null &&
              (targetsLoaded ? (
                <ContractForm
                  key={shown.id}
                  companyId={companyId}
                  contract={shown}
                  targets={targets}
                  pending={saving}
                  error={formError}
                  onSubmit={(input) => void handleSubmit(input)}
                  onCancel={closeSheet}
                />
              ) : targetsError === null ? (
                <p role="status" className="status">
                  <Spinner /> Carregando…
                </p>
              ) : (
                <p role="alert">{targetsError}</p>
              ))}
          </Sheet>
        )}
      </div>
    </>
  );
}

function statusClass(status: ContractStatus): string {
  switch (status) {
    case "active":
      return "badge badge-dot badge-ok";
    case "terminated":
      return "badge badge-dot badge-critical";
    default:
      return "badge badge-dot badge-muted";
  }
}

/** A value of the trail as a person reads it: names for references, words for keys. */
function displayValue(field: string, value: string | null, names: Map<string, string>): string {
  if (value === null) return "—";
  if (["customer_id", "seller_id", "payment_term_id", "category_id", "cost_center_id"].includes(field)) {
    return names.get(value) ?? value;
  }
  if (field === "status") return CONTRACT_STATUS_LABELS[value as ContractStatus] ?? value;
  if (field === "cycle") return CYCLE_LABELS[value as BillingCycle] ?? value;
  if (field === "starts_on" || field === "ends_on") return formatDate(value);
  if (field === "lines") {
    return value
      .split("\n")
      .map((line) => {
        // The description comes last and may hold colons of its own.
        const [product, quantity, price, discount, ...description] = line.split(":");
        const name = description.length > 0 ? description.join(":") : (names.get(product) ?? "item");
        return `${name} × ${quantity.replace(".", ",")} a ${formatMoney(price)}${discount === "0" ? "" : ` (− ${formatMoney(discount)})`}`;
      })
      .join("; ");
  }
  return value;
}
