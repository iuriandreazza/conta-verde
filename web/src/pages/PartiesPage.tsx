import { Fragment, useState } from "react";
import { useParams } from "react-router-dom";
import {
  NO_FILTERS,
  api,
  errorMessage,
  type Party,
  type PartyFilters,
  type PartyInput,
} from "../api";
import { PlusIcon } from "../components/icons/plus";
import { Sheet } from "../components/Sheet";
import { useCreation, useOnCreated } from "../creation/CreationProvider";
import { PageHeader } from "../components/PageHeader";
import { DocumentBadge } from "../components/DocumentBadge";
import { ListTools } from "../components/ListTools";
import { ListRow } from "../components/ListRow";
import { Notice } from "../components/Notice";
import { PartyForm } from "../components/PartyForm";
import { Button } from "../components/Button";
import { CsvImport } from "../components/CsvImport";
import { ChevronDownIcon } from "../components/icons/chevron-down";
import { HistoryIcon } from "../components/icons/history";
import { RefreshCWIcon } from "../components/icons/refresh-cw";
import { UsersIcon } from "../components/icons/users";
import { PartyHistory } from "../components/PartyHistory";
import { Spinner } from "../components/Spinner";
import { msg, useI18n, type Translate } from "../i18n";
import { registryNoticeFor, type RegistryNotice } from "../lib/registryNotice";
import { useLastValue } from "../lib/useLastValue";
import { usePagedList } from "../lib/usePagedList";
import { useCanWrite } from "../lib/useCanWrite";
import { SquarePenIcon } from "../components/icons/square-pen";

const PARTY_COLUMNS = [msg("Name"), msg("Document"), msg("E-mail"), msg("Roles"), msg("Tags")];

function partyRow(party: Party, t: Translate): string[] {
  const roles = [
    party.is_customer && t("Customer"),
    party.is_supplier && t("Supplier"),
    party.is_carrier && t("Carrier"),
    party.is_seller && t("Seller"),
  ].filter(Boolean);
  return [party.name, party.document ?? "", party.email ?? "", roles.join(", "), party.tags.join(", ")];
}

// Layout remounts the page whenever the path changes, so `companyId` never changes within one
// instance: the old list is dropped and a reply still in flight for another company lands on an
// unmounted component.
export function PartiesPage() {
  const { t, rich } = useI18n();
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("parties");
  const [editing, setEditing] = useState<Party | null>(null);
  const [saving, setSaving] = useState(false);
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
  const isFiltered = applied.q !== "" || applied.kind !== "" || applied.tag !== "";

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
      setVerifyNotice(registryNoticeFor(updated, "rechecking", t));
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setVerifyingId(null);
    }
  }

  const { openCreate } = useCreation();
  const shown = useLastValue(editing);
  // A party registered from the top bar or from here: read the list again and flash the new row.
  useOnCreated("party", (created) => {
    // What an earlier change or re-check said is not about this one.
    setRegistryNotice(null);
    setVerifyNotice(null);
    setHighlightedId(created.id);
    void loadFirstPage();
  });

  function closeSheet() {
    // A change on its way finishes first: closing now would hide what a failure has to say.
    if (saving) return;
    setEditing(null);
    setFormError(null);
  }

  async function handleSubmit(input: PartyInput) {
    if (editing === null) return;
    setSaving(true);
    setFormError(null);
    setRegistryNotice(null);
    // A message about an earlier re-check would read like a statement about the new state.
    setVerifyNotice(null);
    try {
      const updated = await api.updateParty(companyId, editing.id, input);
      // In place: the list keeps its page and its filters, but a party that stopped being of the
      // kind shown, or lost the tag shown, no longer belongs in it.
      const tag = applied.tag.trim().toLowerCase();
      const stillShown = (item: Party) =>
        (applied.kind === "" || item[`is_${applied.kind}` as const]) &&
        (tag === "" || item.tags.includes(tag));
      setParties((current) =>
        current.flatMap((item) => (item.id !== updated.id ? [item] : stillShown(updated) ? [updated] : [])),
      );
      setHighlightedId(updated.id);
      // The registry is only asked about a document that changed: otherwise there is nothing new.
      setRegistryNotice(updated.document !== editing.document ? registryNoticeFor(updated, "updating", t) : null);
      setEditing(null);
    } catch (failure) {
      setFormError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  return (
    <>
      <PageHeader title={t("Customers and suppliers")} description={t("Who buys from you and who you buy from.")} />
      <div className="list-actions">
        {canWrite && (
          <Button type="button" icon={PlusIcon} onClick={() => openCreate("party")}>
            {t("New customer or supplier")}
          </Button>
        )}
        <ListTools
          filename="clientes-e-fornecedores"
          header={PARTY_COLUMNS.map((column) => t(column))}
          fetchPage={(after) => api.listParties(companyId, applied, after)}
          toRow={(party) => partyRow(party, t)}
        />
      </div>
      <Notice message={registryNotice?.message ?? null} tone={registryNotice?.tone} />
      <div>
        <section className="card list-card">
        <h2>{t("Registered")}</h2>
        <div className="filters" role="search">
          <label>
            {t("Search")}
            <input
              type="search"
              value={filters.q}
              placeholder={t("Name, document or e-mail")}
              autoComplete="off"
              maxLength={100}
              onChange={(event) => setFilters({ ...filters, q: event.target.value })}
            />
          </label>
          <label>
            {t("Show")}
            <select
              value={filters.kind}
              onChange={(event) =>
                setFilters({ ...filters, kind: event.target.value as PartyFilters["kind"] })
              }
            >
              <option value="">{t("All")}</option>
              <option value="customer">{t("Customers")}</option>
              <option value="supplier">{t("Suppliers")}</option>
              <option value="carrier">{t("Carriers")}</option>
              <option value="seller">{t("Sellers")}</option>
            </select>
          </label>
          <label>
            {t("Tag")}
            <input
              type="search"
              value={filters.tag}
              placeholder={t("For example vip")}
              autoComplete="off"
              maxLength={30}
              // No comma: a tag never has one, and the API would answer 400 to it.
              onChange={(event) => setFilters({ ...filters, tag: event.target.value.replace(/,/g, "") })}
            />
          </label>
          <label>
            {t("Order")}
            <select
              value={filters.sort}
              onChange={(event) =>
                setFilters({ ...filters, sort: event.target.value as PartyFilters["sort"] })
              }
            >
              <option value="name">{t("Name (A–Z)")}</option>
              <option value="-name">{t("Name (Z–A)")}</option>
            </select>
          </label>
        </div>
        <Notice message={error} />
        <Notice message={verifyNotice?.message ?? null} tone={verifyNotice?.tone} />
        {loading ? (
          <p role="status" className="status">
            <Spinner /> {t("Loading…")}
          </p>
        ) : parties.length === 0 && error ? (
          // The list could not be loaded at all: offer a retry instead of a table with no rows.
          <Button type="button" variant="ghost" onClick={() => void loadFirstPage()}>
            {t("Try again")}
          </Button>
        ) : parties.length === 0 && isFiltered ? (
          <div className="empty">
            <UsersIcon size={40} aria-hidden />
            <p role="status">{t("No records found.")}</p>
            <Button type="button" variant="ghost" onClick={() => setFilters({ ...NO_FILTERS, sort: filters.sort })}>
              {t("Clear filters")}
            </Button>
          </div>
        ) : parties.length === 0 ? (
          <div className="empty">
            <UsersIcon size={40} aria-hidden />
            <p>{t("No records yet.")}</p>
            <p className="muted">{t("Use the form to add the first one.")}</p>
          </div>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>{t("Name")}</th>
                  <th>{t("Document")}</th>
                  <th>{t("E-mail")}</th>
                  <th>
                    <span className="visually-hidden">{t("Actions")}</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {parties.map((party, index) => (
                  <Fragment key={party.id}>
                  <ListRow index={index} highlighted={party.id === highlightedId}>
                    <td>
                      <div className="cell-title">{party.name}</div>
                      <div className="muted">
                        {[
                          party.is_customer && t("Customer"),
                          party.is_supplier && t("Supplier"),
                          party.is_carrier && t("Carrier"),
                          party.is_seller && t("Seller"),
                        ]
                          .filter(Boolean)
                          .join(" · ")}
                      </div>
                      {party.address && (
                        <div className="muted">{`${party.address.city}/${party.address.state}`}</div>
                      )}
                      {party.tags.length > 0 && (
                        <div className="tag-list">
                          {party.tags.map((tag) => (
                            <button
                              key={tag}
                              type="button"
                              className="badge badge-muted"
                              aria-label={t("Filter by tag {tag}", { tag })}
                              onClick={() => setFilters({ ...filters, tag })}
                            >
                              {tag}
                            </button>
                          ))}
                        </div>
                      )}
                    </td>
                    <td>
                      {party.document ?? "—"} <DocumentBadge check={party.document_check} />
                      {canWrite && party.document_check.needs_attention && (
                        <Button
                          iconOnly
                          type="button"
                          variant="ghost"
                          aria-label={t("Check the CNPJ of {name} again", { name: party.name })}
                          icon={RefreshCWIcon}
                          pending={verifyingId === party.id}
                          disabled={verifyingId !== null || saving}
                          onClick={() => void verifyAgain(party)}
                        >
                          {t("Check again")}
                        </Button>
                      )}
                    </td>
                    <td>
                      {party.email ?? "—"}
                      {party.contacts.length > 0 && (
                        <div className="muted">
                          {party.contacts.length === 1 ? t("1 contact") : t("{count} contacts", { count: party.contacts.length })}
                        </div>
                      )}
                    </td>
                    <td className="row-actions">
                      {canWrite && (
                        <Button
                          iconOnly
                          icon={SquarePenIcon}
                          type="button"
                          variant="ghost"
                          aria-label={t("Edit {name}", { name: party.name })}
                          disabled={saving}
                          onClick={() => {
                            setEditing(party);
                            setFormError(null);
                            setRegistryNotice(null);
                          }}
                        >
                          {t("Edit")}
                        </Button>
                      )}
                      <Button
                        iconOnly
                        type="button"
                        variant="ghost"
                        aria-label={t("History of {name}", { name: party.name })}
                        aria-expanded={historyOpenFor === party.id}
                        aria-controls={`history-${party.id}`}
                        icon={HistoryIcon}
                        onClick={() =>
                          setHistoryOpenFor(historyOpenFor === party.id ? null : party.id)
                        }
                      >
                        {t("History")}
                      </Button>
                    </td>
                  </ListRow>
                  {historyOpenFor === party.id && (
                    <tr id={`history-${party.id}`}>
                      <td colSpan={5} role="region" aria-label={t("History of {name}", { name: party.name })}>
                        <PartyHistory
                          // Remounted when the party changes (a re-check, an edit), so an open
                          // history follows it.
                          key={JSON.stringify(party)}
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
            {t("Load more")}
          </Button>
        )}
      </section>
        {canWrite && (
          <Sheet open={editing !== null} locked={saving} title={t("Edit {name}", { name: shown?.name ?? "" })} onClose={closeSheet}>
            {shown !== null && (
              <PartyForm
                key={shown.id}
                party={shown}
                pending={saving}
                blocked={verifyingId !== null}
                error={formError}
                lookupAddress={(postalCode) => api.lookupPostalCode(companyId, postalCode)}
                onSubmit={(input) => void handleSubmit(input)}
                onCancel={closeSheet}
              />
            )}
          </Sheet>
        )}
      </div>
      {canWrite && (
        <CsvImport
          title={t("Import from a CSV file")}
          template={PARTIES_TEMPLATE}
          templateName="clientes-e-fornecedores.csv"
          run={(file, dryRun) => api.importParties(companyId, file, dryRun)}
          onImported={() => void loadFirstPage()}
        >
          <p>
            {rich(
              "The first line names the columns, in any order; a comma or a semicolon separates them. Only {name} is required, and at least one of {customer}, {supplier}, {carrier} and {seller} says what the record is ({yes} or {no}). The address is {postal_code}, {street}, {number}, {complement}, {district}, {city} and {state}; tags go in {tags}, separated by {pipe}. Contacts are added later, one by one.",
              {
                name: <code>name</code>,
                customer: <code>customer</code>,
                supplier: <code>supplier</code>,
                carrier: <code>carrier</code>,
                seller: <code>seller</code>,
                yes: <code>sim</code>,
                no: <code>não</code>,
                postal_code: <code>postal_code</code>,
                street: <code>street</code>,
                number: <code>number</code>,
                complement: <code>complement</code>,
                district: <code>district</code>,
                city: <code>city</code>,
                state: <code>state</code>,
                tags: <code>tags</code>,
                pipe: <code>|</code>,
              },
            )}
          </p>
          <p>
            {t("Nothing is imported while a row has a problem. Anyone with the same document is skipped, never overwritten. CNPJs are not checked with the federal registry: use \"Check again\" afterwards.")}
          </p>
        </CsvImport>
      )}
    </>
  );
}

const PARTIES_TEMPLATE = [
  "name,document,email,customer,supplier,carrier,seller,postal_code,street,number,complement,district,city,state,tags",
  "Padaria Central,11.222.333/0001-81,contato@padaria.example,sim,não,não,não,01310-100,Avenida Paulista,1000,,Bela Vista,São Paulo,SP,vip|atacado",
  "",
].join("\n");

