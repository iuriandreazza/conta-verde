import { Fragment, useCallback, useEffect, useMemo, useState, type FormEvent } from "react";
import { Link, useParams } from "react-router-dom";
import {
  NO_FILTERS,
  NO_PAYMENT_TERM_FILTERS,
  NO_PRODUCT_FILTERS,
  NO_QUOTE_FILTERS,
  api,
  errorMessage,
  type Category,
  type CostCenter,
  type Party,
  type Sale,
  type PaymentTerm,
  type Product,
  type Quote,
  type QuoteFilters,
  type QuoteInput,
  type QuoteStatus,
  type SentQuote,
} from "../api";
import { Button } from "../components/Button";
import { History } from "../components/History";
import { CheckIcon } from "../components/icons/check";
import { ChevronDownIcon } from "../components/icons/chevron-down";
import { FileTextIcon } from "../components/icons/file-text";
import { HistoryIcon } from "../components/icons/history";
import { PlusIcon } from "../components/icons/plus";
import { HandCoinsIcon } from "../components/icons/hand-coins";
import { SendIcon } from "../components/icons/send";
import { SquarePenIcon } from "../components/icons/square-pen";
import { LinesEditor, linesProblem, linesToInput, newLine, rowsFromLines, type LineRow } from "../components/LinesEditor";
import { ListRow } from "../components/ListRow";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { Spinner } from "../components/Spinner";
import { treeRows } from "../lib/categoryTree";
import { formatMoney } from "../lib/decimal";
import { formatDate, isIsoDate } from "../lib/entryInstallments";
import { QUOTE_STATUS_LABELS } from "../lib/quoteStatus";
import { readUpTo } from "../lib/readUpTo";
import { todayIso } from "../lib/settlement";
import { useCanWrite } from "../lib/useCanWrite";
import { usePagedList } from "../lib/usePagedList";

const FIELD_LABELS: Record<string, string> = {
  customer_id: "Cliente",
  issued_on: "Emitido em",
  valid_until: "Válido até",
  payment_term_id: "Prazo de pagamento",
  notes: "Observações",
  lines: "Itens",
  status: "Situação",
  link: "Link",
  answered_by: "Respondido por",
};

/** A quote is open to change while it is a draft or waiting for the answer. */
const isOpen = (quote: Quote) => quote.status === "draft" || quote.status === "sent";

/** `days` after an ISO day, written as one. */
function plusDays(iso: string, days: number): string {
  const [year, month, day] = iso.split("-").map(Number);
  return todayIso(new Date(year, month - 1, day + days));
}

// Layout remounts the page whenever the path changes, so `companyId` never changes within one instance.
export function QuotesPage() {
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("sales");
  const [filters, setFilters] = useState<QuoteFilters>(NO_QUOTE_FILTERS);
  const [editing, setEditing] = useState<Quote | null>(null);
  // Bumped after a save so the form starts over, empty or for another quote.
  const [formVersion, setFormVersion] = useState(0);
  const [rows, setRows] = useState<LineRow[]>(() => [newLine()]);
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [highlightedId, setHighlightedId] = useState<string | null>(null);
  const [historyOpenFor, setHistoryOpenFor] = useState<string | null>(null);
  const [sendingFor, setSendingFor] = useState<string | null>(null);
  const [sending, setSending] = useState(false);
  const [sendError, setSendError] = useState<string | null>(null);
  const [sent, setSent] = useState<SentQuote | null>(null);
  const [parties, setParties] = useState<Party[]>([]);
  const [products, setProducts] = useState<Product[]>([]);
  const [terms, setTerms] = useState<PaymentTerm[]>([]);
  const [categories, setCategories] = useState<Category[]>([]);
  const [costCenters, setCostCenters] = useState<CostCenter[]>([]);
  const [convertingFor, setConvertingFor] = useState<string | null>(null);
  const [converting, setConverting] = useState(false);
  const [convertError, setConvertError] = useState<string | null>(null);
  const [converted, setConverted] = useState<Sale | null>(null);
  const [targetsError, setTargetsError] = useState<string | null>(null);
  const [targetsLoaded, setTargetsLoaded] = useState(false);

  // What a quote points at is read once: the form needs it, and so do the names in the list.
  useEffect(() => {
    let current = true;
    Promise.all([
      readUpTo((after) => api.listParties(companyId, NO_FILTERS, after)),
      readUpTo((after) => api.listProducts(companyId, NO_PRODUCT_FILTERS, after)),
      readUpTo((after) => api.listPaymentTerms(companyId, NO_PAYMENT_TERM_FILTERS, after)),
      api.listCategories(companyId),
      readUpTo((after) => api.listCostCenters(companyId, { q: "", active: "", sort: "name" }, after)),
    ])
      .then(([partyList, productList, termList, categoryList, centerList]) => {
        if (current) {
          setParties(partyList);
          setProducts(productList);
          setTerms(termList);
          setCategories(categoryList.items);
          setCostCenters(centerList);
          setTargetsLoaded(true);
        }
      })
      .catch((failure: unknown) => current && setTargetsError(errorMessage(failure)));
    return () => {
      current = false;
    };
  }, [companyId]);

  const names = useMemo(() => {
    const byId = new Map<string, string>();
    for (const party of parties) byId.set(party.id, party.name);
    for (const term of terms) byId.set(term.id, term.name);
    for (const product of products) byId.set(product.id, product.name);
    return byId;
  }, [parties, terms, products]);

  const { items, setItems, nextCursor, loading, loadingMore, error, applied, reload, loadMore } =
    usePagedList<Quote, QuoteFilters>((applied, after) => api.listQuotes(companyId, applied, after), filters);
  const isFiltered = applied.q !== "" || applied.status !== "";

  // What is in use, plus what the quote already points at, so changing one whose product was
  // deactivated since still shows it.
  const inQuote = new Set(rows.map((row) => row.product_id));
  const productChoices = products.filter((product) => product.active || inQuote.has(product.id));
  const termChoices = terms.filter((term) => term.active || term.id === editing?.payment_term_id);

  function startEditing(quote: Quote | null) {
    setEditing(quote);
    setRows(quote === null ? [newLine()] : rowsFromLines(quote.lines));
    setFormError(null);
    setSaved(null);
    setFormVersion((version) => version + 1);
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (saving) {
      return;
    }
    const form = new FormData(event.currentTarget);
    const field = (name: string) => String(form.get(name) ?? "").trim();
    if (!isIsoDate(field("issued_on")) || !isIsoDate(field("valid_until"))) {
      setFormError("Informe a data de emissão e a validade.");
      return;
    }
    if (field("valid_until") < field("issued_on")) {
      setFormError("A validade não pode ser antes da emissão.");
      return;
    }
    const problem = linesProblem(rows);
    if (problem !== null) {
      setFormError(problem);
      return;
    }
    const input: QuoteInput = {
      customer_id: field("customer_id"),
      issued_on: field("issued_on"),
      valid_until: field("valid_until"),
      payment_term_id: field("payment_term_id") || undefined,
      notes: field("notes") || undefined,
      lines: linesToInput(rows),
    };
    setSaving(true);
    setFormError(null);
    setSaved(null);
    try {
      if (editing === null) {
        const created = await api.createQuote(companyId, input);
        setHighlightedId(created.id);
        startEditing(null);
        setSaved(`Orçamento nº ${created.number} criado.`);
        await reload();
      } else {
        const updated = await api.updateQuote(companyId, editing.id, input);
        // In place: the list keeps its page and its filters.
        setItems((current) => current.map((item) => (item.id === updated.id ? updated : item)));
        startEditing(null);
        setSaved(
          editing.status === "sent" && updated.status === "draft"
            ? `Orçamento nº ${updated.number} atualizado. Voltou a ser rascunho e o link enviado deixou de valer: envie de novo.`
            : `Orçamento nº ${updated.number} atualizado.`,
        );
      }
    } catch (failure) {
      setFormError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  async function handleConvert(event: FormEvent<HTMLFormElement>, quote: Quote) {
    event.preventDefault();
    if (converting) {
      return;
    }
    const form = new FormData(event.currentTarget);
    const field = (name: string) => String(form.get(name) ?? "").trim();
    if (!isIsoDate(field("sold_on"))) {
      setConvertError("Informe a data da venda.");
      return;
    }
    setConverting(true);
    setConvertError(null);
    try {
      const sale = await api.convertQuote(companyId, quote.id, {
        sold_on: field("sold_on"),
        category_id: field("category_id"),
        cost_center_id: field("cost_center_id") || undefined,
        seller_id: field("seller_id") || undefined,
      });
      setItems((current) => current.map((item) => (item.id === quote.id ? { ...item, status: "converted" } : item)));
      setConverted(sale);
    } catch (failure) {
      setConvertError(errorMessage(failure));
    } finally {
      setConverting(false);
    }
  }

  async function handleSend(event: FormEvent<HTMLFormElement>, quote: Quote) {
    event.preventDefault();
    if (sending) {
      return;
    }
    const to = String(new FormData(event.currentTarget).get("to") ?? "").trim();
    setSending(true);
    setSendError(null);
    try {
      const result = await api.sendQuote(companyId, quote.id, to);
      setItems((current) => current.map((item) => (item.id === result.quote.id ? result.quote : item)));
      setSent(result);
    } catch (failure) {
      setSendError(errorMessage(failure));
    } finally {
      setSending(false);
    }
  }

  const historyLoader = useCallback((id: string) => () => api.quoteHistory(companyId, id), [companyId]);

  return (
    <>
      <PageHeader
        title="Orçamentos"
        description="Itens com desconto, validade e um link para o cliente aprovar. Aprovado, vira venda."
      />
      <div className={canWrite ? "split" : "split split-single"}>
        <section className="card list-card">
          <h2>Emitidos</h2>
          <div className="filters" role="search">
            <label>
              Buscar
              <input
                type="search"
                value={filters.q}
                placeholder="Número, item ou observação"
                autoComplete="off"
                maxLength={100}
                onChange={(event) => setFilters({ ...filters, q: event.target.value })}
              />
            </label>
            <label>
              Situação
              <select
                value={filters.status}
                onChange={(event) => setFilters({ ...filters, status: event.target.value as QuoteFilters["status"] })}
              >
                <option value="">Todas</option>
                {Object.entries(QUOTE_STATUS_LABELS).map(([key, label]) => (
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
              <FileTextIcon size={40} aria-hidden />
              <p role="status">Nenhum orçamento encontrado.</p>
              <Button type="button" variant="ghost" onClick={() => setFilters({ ...NO_QUOTE_FILTERS })}>
                Limpar filtros
              </Button>
            </div>
          ) : items.length === 0 ? (
            <div className="empty">
              <FileTextIcon size={40} aria-hidden />
              <p>Nenhum orçamento ainda.</p>
              <p className="muted">Monte os itens, envie o link e o cliente aprova sem precisar de conta.</p>
            </div>
          ) : (
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Nº</th>
                    <th>Cliente</th>
                    <th>Validade</th>
                    <th className="num">Total</th>
                    <th>Situação</th>
                    <th>
                      <span className="visually-hidden">Ações</span>
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {items.map((quote, index) => (
                    <Fragment key={quote.id}>
                      <ListRow index={index} highlighted={quote.id === highlightedId}>
                        <td>{quote.number}</td>
                        <td>
                          <div className="cell-title">{names.get(quote.customer_id) ?? ""}</div>
                          <div className="muted">
                            {quote.lines.length} {quote.lines.length === 1 ? "item" : "itens"}
                          </div>
                        </td>
                        <td>{formatDate(quote.valid_until)}</td>
                        <td className="num">{formatMoney(quote.total)}</td>
                        <td>
                          <span className={statusClass(quote.status)}>{QUOTE_STATUS_LABELS[quote.status]}</span>
                        </td>
                        <td className="row-actions">
                          <a
                            className="btn btn-ghost btn-icon"
                            href={api.quotePdfUrl(companyId, quote.id)}
                            target="_blank"
                            rel="noreferrer"
                            aria-label={`PDF do orçamento ${quote.number}`}
                            title={`PDF do orçamento ${quote.number}`}
                          >
                            <FileTextIcon size={16} aria-hidden />
                          </a>
                          {canWrite && quote.status !== "declined" && quote.status !== "converted" && (
                            <Button
                              iconOnly
                              icon={HandCoinsIcon}
                              type="button"
                              variant="ghost"
                              aria-label={`Converter o orçamento ${quote.number} em venda`}
                              aria-expanded={convertingFor === quote.id}
                              onClick={() => {
                                setConvertingFor(convertingFor === quote.id ? null : quote.id);
                                setConvertError(null);
                                setConverted(null);
                              }}
                            >
                              Converter em venda
                            </Button>
                          )}
                          {canWrite && isOpen(quote) && (
                            <>
                              <Button
                                iconOnly
                                icon={SquarePenIcon}
                                type="button"
                                variant="ghost"
                                aria-label={`Alterar o orçamento ${quote.number}`}
                                disabled={saving}
                                onClick={() => startEditing(quote)}
                              >
                                Alterar
                              </Button>
                              <Button
                                iconOnly
                                icon={SendIcon}
                                type="button"
                                variant="ghost"
                                aria-label={`Enviar o orçamento ${quote.number}`}
                                aria-expanded={sendingFor === quote.id}
                                onClick={() => {
                                  setSendingFor(sendingFor === quote.id ? null : quote.id);
                                  setSendError(null);
                                  setSent(null);
                                }}
                              >
                                Enviar
                              </Button>
                            </>
                          )}
                          <Button
                            iconOnly
                            type="button"
                            variant="ghost"
                            aria-label={`Histórico do orçamento ${quote.number}`}
                            aria-expanded={historyOpenFor === quote.id}
                            aria-controls={`history-${quote.id}`}
                            icon={HistoryIcon}
                            onClick={() => setHistoryOpenFor(historyOpenFor === quote.id ? null : quote.id)}
                          >
                            Histórico
                          </Button>
                        </td>
                      </ListRow>
                      {convertingFor === quote.id && (
                        <tr>
                          <td colSpan={6} role="region" aria-label={`Conversão do orçamento ${quote.number}`}>
                            {converted !== null && converted.quote_id === quote.id ? (
                              <Notice
                                tone="success"
                                message={`Pedido nº ${converted.number} criado a partir deste orçamento. Conclua-o em Vendas para gerar a conta a receber.`}
                              />
                            ) : (
                              <form className="opening-form" onSubmit={(event) => void handleConvert(event, quote)}>
                                <label>
                                  Data da venda
                                  <input name="sold_on" type="date" required defaultValue={todayIso()} />
                                </label>
                                <label>
                                  Categoria da receita
                                  <select name="category_id" required defaultValue="">
                                    <option value="" disabled>
                                      Escolha
                                    </option>
                                    {treeRows(categories)
                                      .filter(({ category }) => category.active && category.dre_group !== "transfers")
                                      .map(({ category, depth }) => (
                                        <option key={category.id} value={category.id}>
                                          {"— ".repeat(depth)}
                                          {category.name}
                                        </option>
                                      ))}
                                  </select>
                                </label>
                                <label>
                                  Centro de custo
                                  <select name="cost_center_id" defaultValue="">
                                    <option value="">Nenhum</option>
                                    {costCenters
                                      .filter((center) => center.active)
                                      .map((center) => (
                                        <option key={center.id} value={center.id}>
                                          {center.name}
                                        </option>
                                      ))}
                                  </select>
                                </label>
                                <label>
                                  Vendedor
                                  <select name="seller_id" defaultValue="">
                                    <option value="">Nenhum</option>
                                    {parties
                                      .filter((party) => party.is_seller)
                                      .map((party) => (
                                        <option key={party.id} value={party.id}>
                                          {party.name}
                                        </option>
                                      ))}
                                  </select>
                                </label>
                                <Button type="submit" icon={HandCoinsIcon} pending={converting}>
                                  Criar pedido
                                </Button>
                                <Notice message={convertError} />
                              </form>
                            )}
                            {converted !== null && converted.quote_id === quote.id && (
                              <p>
                                <Link to={`/companies/${companyId}/sales`}>Ir para as vendas</Link>
                              </p>
                            )}
                          </td>
                        </tr>
                      )}
                      {sendingFor === quote.id && (
                        <tr>
                          <td colSpan={6} role="region" aria-label={`Envio do orçamento ${quote.number}`}>
                            <form className="opening-form" onSubmit={(event) => void handleSend(event, quote)}>
                              <label>
                                E-mail do cliente
                                <input
                                  name="to"
                                  type="email"
                                  required
                                  autoComplete="off"
                                  defaultValue={parties.find((party) => party.id === quote.customer_id)?.email ?? ""}
                                />
                              </label>
                              <Button type="submit" icon={SendIcon} pending={sending}>
                                Enviar link
                              </Button>
                              <Notice message={sendError} />
                              {sent !== null && sent.quote.id === quote.id && (
                                <>
                                  <Notice
                                    tone={sent.emailed ? "success" : "warning"}
                                    message={
                                      sent.emailed
                                        ? "Link enviado por e-mail. Aparece só aqui: copie se precisar."
                                        : "O e-mail não saiu. Passe o link ao cliente por outro meio: ele aparece só aqui."
                                    }
                                  />
                                  <input aria-label="Link para o cliente" readOnly value={sent.link} onFocus={(event) => event.target.select()} />
                                </>
                              )}
                            </form>
                          </td>
                        </tr>
                      )}
                      {historyOpenFor === quote.id && (
                        <tr id={`history-${quote.id}`}>
                          <td colSpan={6} role="region" aria-label={`Histórico do orçamento ${quote.number}`}>
                            <History
                              load={historyLoader(quote.id)}
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
          <aside className="card form-card">
            <h2>{editing === null ? "Novo orçamento" : `Alterar o orçamento ${editing.number}`}</h2>
            {targetsLoaded && (
              <form
                // A new key starts the form over for another quote, or an empty one after a save.
                key={`${editing?.id ?? "new"}-${formVersion}`}
                onSubmit={(event) => void handleSubmit(event)}
              >
                <label>
                  Cliente
                  <select name="customer_id" required defaultValue={editing?.customer_id ?? ""}>
                    <option value="" disabled>
                      Escolha um cliente
                    </option>
                    {parties.map((party) => (
                      <option key={party.id} value={party.id}>
                        {party.name}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  Emitido em
                  <input name="issued_on" type="date" required defaultValue={editing?.issued_on ?? todayIso()} />
                </label>
                <label>
                  Válido até
                  <input
                    name="valid_until"
                    type="date"
                    required
                    defaultValue={editing?.valid_until ?? plusDays(todayIso(), 15)}
                  />
                </label>
                <label>
                  Prazo de pagamento
                  <select name="payment_term_id" defaultValue={editing?.payment_term_id ?? ""}>
                    <option value="">Não informar</option>
                    {termChoices.map((term) => (
                      <option key={term.id} value={term.id}>
                        {term.name}
                      </option>
                    ))}
                  </select>
                </label>
                <LinesEditor rows={rows} onChange={setRows} products={productChoices} />
                <label>
                  Observações
                  <textarea name="notes" rows={3} maxLength={2000} defaultValue={editing?.notes ?? ""} />
                </label>
                {editing?.status === "sent" && (
                  <p className="muted">
                    Alterar um orçamento enviado o devolve a rascunho e o link que o cliente recebeu deixa de valer.
                  </p>
                )}
                <Notice message={formError} />
                <div className="form-actions">
                  <Button type="submit" icon={editing === null ? PlusIcon : CheckIcon} pending={saving}>
                    {editing === null ? "Criar orçamento" : "Salvar alterações"}
                  </Button>
                  {editing !== null && (
                    <Button type="button" variant="ghost" disabled={saving} onClick={() => startEditing(null)}>
                      Cancelar
                    </Button>
                  )}
                </div>
              </form>
            )}
            <Notice message={saved} tone="success" />
          </aside>
        )}
      </div>
    </>
  );
}

function statusClass(status: QuoteStatus): string {
  switch (status) {
    case "approved":
    case "converted":
      return "badge badge-dot badge-ok";
    case "declined":
      return "badge badge-dot badge-critical";
    case "sent":
      return "badge badge-dot badge-warning";
    default:
      return "badge badge-dot badge-muted";
  }
}

/** A value of the trail as a person reads it: names for references, words for keys. */
function displayValue(field: string, value: string | null, names: Map<string, string>): string {
  if (value === null) return "—";
  if (field === "customer_id" || field === "payment_term_id") return names.get(value) ?? value;
  if (field === "status") return QUOTE_STATUS_LABELS[value as QuoteStatus] ?? value;
  if (field === "issued_on" || field === "valid_until") return formatDate(value);
  if (field === "lines") {
    return value
      .split(",")
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
