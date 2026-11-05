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
import { PageHeader } from "../components/PageHeader";
import { DocumentBadge } from "../components/DocumentBadge";
import { ListRow } from "../components/ListRow";
import { Notice, type NoticeTone } from "../components/Notice";
import { PartyForm } from "../components/PartyForm";
import { Button } from "../components/Button";
import { CsvImport } from "../components/CsvImport";
import { ChevronDownIcon } from "../components/icons/chevron-down";
import { HistoryIcon } from "../components/icons/history";
import { RefreshCWIcon } from "../components/icons/refresh-cw";
import { UsersIcon } from "../components/icons/users";
import { PartyHistory } from "../components/PartyHistory";
import { Spinner } from "../components/Spinner";
import { usePagedList } from "../lib/usePagedList";
import { useCanWrite } from "../lib/useCanWrite";
import { SquarePenIcon } from "../components/icons/square-pen";

// Layout remounts the page whenever the path changes, so `companyId` never changes within one
// instance: the old list is dropped and a reply still in flight for another company lands on an
// unmounted component.
export function PartiesPage() {
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("parties");
  const [editing, setEditing] = useState<Party | null>(null);
  // Bumped after a save so the form starts over, empty or for another party.
  const [formVersion, setFormVersion] = useState(0);
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
      setVerifyNotice(registryNoticeFor(updated, "rechecking"));
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setVerifyingId(null);
    }
  }

  async function handleSubmit(input: PartyInput) {
    setSaving(true);
    setFormError(null);
    setRegistryNotice(null);
    // A message about an earlier re-check would read like a statement about the new state.
    setVerifyNotice(null);
    try {
      if (editing === null) {
        const created = await api.createParty(companyId, input);
        setHighlightedId(created.id);
        setRegistryNotice(registryNoticeFor(created, "registering"));
        setFormVersion((version) => version + 1);
        await loadFirstPage();
      } else {
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
        setRegistryNotice(
          updated.document !== editing.document ? registryNoticeFor(updated, "updating") : null,
        );
        setEditing(null);
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
      <PageHeader title="Clientes e fornecedores" description="Quem compra de você e de quem você compra." />
      <div className={canWrite ? "split" : "split split-single"}>
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
              <option value="carrier">Transportadoras</option>
              <option value="seller">Vendedores</option>
            </select>
          </label>
          <label>
            Etiqueta
            <input
              type="search"
              value={filters.tag}
              placeholder="Por exemplo vip"
              autoComplete="off"
              maxLength={30}
              // No comma: a tag never has one, and the API would answer 400 to it.
              onChange={(event) => setFilters({ ...filters, tag: event.target.value.replace(/,/g, "") })}
            />
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
                  <th>
                    <span className="visually-hidden">Ações</span>
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
                          party.is_customer && "Cliente",
                          party.is_supplier && "Fornecedor",
                          party.is_carrier && "Transportadora",
                          party.is_seller && "Vendedor",
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
                              aria-label={`Filtrar pela etiqueta ${tag}`}
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
                          className="btn-small"
                          aria-label={`Verificar de novo o CNPJ de ${party.name}`}
                          icon={RefreshCWIcon}
                          pending={verifyingId === party.id}
                          disabled={verifyingId !== null || saving}
                          onClick={() => void verifyAgain(party)}
                        >
                          Verificar de novo
                        </Button>
                      )}
                    </td>
                    <td>
                      {party.email ?? "—"}
                      {party.contacts.length > 0 && (
                        <div className="muted">
                          {party.contacts.length === 1 ? "1 contato" : `${party.contacts.length} contatos`}
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
                          className="btn-small"
                          aria-label={`Alterar ${party.name}`}
                          disabled={saving}
                          onClick={() => {
                            setEditing(party);
                            setFormError(null);
                            setRegistryNotice(null);
                          }}
                        >
                          Alterar
                        </Button>
                      )}
                      <Button
                        iconOnly
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
            Carregar mais
          </Button>
        )}
      </section>
        {canWrite && (
          <aside className="card form-card">
          <h2>{editing === null ? "Novo cliente ou fornecedor" : `Alterar ${editing.name}`}</h2>
          <PartyForm
            // A new key starts the form over for another party, or an empty one after a save.
            key={`${editing?.id ?? "new"}-${formVersion}`}
            party={editing}
            // After a save or when editing, the person is in the form: put the cursor back in it.
            focusName={editing !== null || formVersion > 0}
            pending={saving}
            blocked={verifyingId !== null}
            error={formError}
            notice={registryNotice}
            lookupAddress={(postalCode) => api.lookupPostalCode(companyId, postalCode)}
            onSubmit={(input) => void handleSubmit(input)}
            onCancel={() => {
              setEditing(null);
              setFormError(null);
            }}
          />
        </aside>
        )}
      </div>
      {canWrite && (
        <CsvImport
          title="Importar de um arquivo CSV"
          template={PARTIES_TEMPLATE}
          templateName="clientes-e-fornecedores.csv"
          run={(file, dryRun) => api.importParties(companyId, file, dryRun)}
          onImported={() => void loadFirstPage()}
        >
          <p>
            A primeira linha dá o nome das colunas, em qualquer ordem; a vírgula ou o ponto e vírgula
            separam. Só <code>name</code> é obrigatória, e ao menos uma de <code>customer</code>,{" "}
            <code>supplier</code>, <code>carrier</code> e <code>seller</code> diz o que o cadastro é
            (<code>sim</code> ou <code>não</code>). O endereço é <code>postal_code</code>,{" "}
            <code>street</code>, <code>number</code>, <code>complement</code>, <code>district</code>,{" "}
            <code>city</code> e <code>state</code>; as etiquetas vão em <code>tags</code>, separadas por{" "}
            <code>|</code>. Contatos se adicionam depois, um a um.
          </p>
          <p>
            Nada é importado enquanto uma linha tiver problema. Quem já tem o mesmo documento é
            ignorado, nunca sobrescrito. Os CNPJs não são consultados na Receita: use "Verificar de
            novo" depois.
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

interface RegistryNotice {
  message: string;
  tone: NoticeTone;
}

// The party is already saved when this runs: the notice only reports what the registry said,
// so a warning never asks the user to redo anything. `after` says which action produced the
// answer, since "Cadastrado" would be wrong for a re-check of an old row.
function registryNoticeFor(
  party: Party,
  after: "registering" | "updating" | "rechecking",
): RegistryNotice | null {
  const check = party.document_check;
  const rechecking = after === "rechecking";
  const done = after === "updating" ? "Atualizado" : "Cadastrado";
  const lead = rechecking ? `${party.name}:` : `${done}, mas`;
  if (check.status === "not_found") {
    return {
      tone: "warning",
      message: `${lead} a Receita não encontrou este CNPJ. Confira o número: ela pode demorar a incluir empresas recém-abertas.`,
    };
  }
  if (check.status === "unverified") {
    return {
      tone: "warning",
      message: rechecking
        ? `${party.name}: a Receita continua indisponível. Tente de novo mais tarde.`
        : `${done}, mas não foi possível consultar o CNPJ na Receita agora. Só os dígitos foram conferidos.`,
    };
  }
  if (check.status === "verified") {
    return check.needs_attention
      ? {
          tone: "warning",
          message: `${rechecking ? `${party.name}:` : `${done}. Atenção:`} ${check.legal_name} está com situação ${check.situation} na Receita.`,
        }
      : {
          tone: "success",
          message: `${rechecking ? `${party.name}: ` : ""}CNPJ verificado: ${check.legal_name}.`,
        };
  }
  return null;
}
