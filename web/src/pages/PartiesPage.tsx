import { motion } from "motion/react";
import { useCallback, useEffect, useState, type FormEvent } from "react";
import { useParams } from "react-router-dom";
import { api, errorMessage, type Party } from "../api";
import { useAuth } from "../auth";
import { Alert } from "../components/Alert";
import { Button } from "../components/Button";
import { ChevronDownIcon } from "../components/icons/chevron-down";
import { PlusIcon } from "../components/icons/plus";
import { UsersIcon } from "../components/icons/users";
import { Spinner } from "../components/Spinner";

// Rows enter one after another, but never wait longer than the eighth for a long page.
const MAX_STAGGERED_ROWS = 8;

export function PartiesPage() {
  const { companyId = "" } = useParams();
  // Keyed by company: switching company remounts the page, which drops the old list and
  // makes any reply still in flight for the previous company land on an unmounted component.
  return <CompanyParties key={companyId} companyId={companyId} />;
}

function CompanyParties({ companyId }: { companyId: string }) {
  const { session } = useAuth();
  const [parties, setParties] = useState<Party[]>([]);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [creating, setCreating] = useState(false);
  const [highlightedId, setHighlightedId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [formError, setFormError] = useState<string | null>(null);

  const companyName =
    session.status === "authenticated"
      ? session.profile.companies.find((company) => company.id === companyId)?.name
      : undefined;

  // Only the very first load shows the spinner; refreshing after an insert keeps the rows on screen.
  const loadFirstPage = useCallback(async () => {
    setError(null);
    try {
      const page = await api.listParties(companyId);
      setParties(page.items);
      setNextCursor(page.next_cursor);
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setLoading(false);
    }
  }, [companyId]);

  useEffect(() => {
    void loadFirstPage();
  }, [loadFirstPage]);

  async function loadMore() {
    if (nextCursor === null || loadingMore) {
      return;
    }
    setLoadingMore(true);
    setError(null);
    try {
      const page = await api.listParties(companyId, nextCursor);
      setParties((current) => [...current, ...page.items]);
      setNextCursor(page.next_cursor);
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setLoadingMore(false);
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
      await loadFirstPage();
    } catch (failure) {
      setFormError(errorMessage(failure));
    } finally {
      setCreating(false);
    }
  }

  return (
    <>
      <h1>{companyName ?? "Clientes e fornecedores"}</h1>

      <section className="card">
        <h2>Novo cliente ou fornecedor</h2>
        <form onSubmit={handleCreate}>
          <label>
            Nome
            <input name="name" required />
          </label>
          <label>
            CPF ou CNPJ
            <input name="document" inputMode="numeric" />
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
          <Alert message={formError} />
          <Button type="submit" icon={PlusIcon} pending={creating}>
            Adicionar
          </Button>
        </form>
      </section>

      <section className="card">
        <h2>Cadastrados</h2>
        <Alert message={error} />
        {loading ? (
          <p role="status" className="status">
            <Spinner /> Carregando…
          </p>
        ) : parties.length === 0 && !error ? (
          <div className="empty">
            <UsersIcon size={40} aria-hidden />
            <p>Nenhum cadastro ainda.</p>
            <p className="muted">Use o formulário acima para adicionar o primeiro.</p>
          </div>
        ) : (
          <table>
            <thead>
              <tr>
                <th>Nome</th>
                <th>Documento</th>
                <th>E-mail</th>
                <th>Tipo</th>
              </tr>
            </thead>
            <tbody>
              {parties.map((party, index) => (
                <motion.tr
                  key={party.id}
                  initial={{ opacity: 0, y: 8, backgroundColor: highlightColor(party, highlightedId, 1) }}
                  animate={{
                    opacity: 1,
                    y: 0,
                    backgroundColor: highlightColor(party, highlightedId, 0),
                  }}
                  transition={{
                    delay: Math.min(index, MAX_STAGGERED_ROWS) * 0.03,
                    duration: party.id === highlightedId ? 1.4 : 0.2,
                  }}
                >
                  <td>{party.name}</td>
                  <td>{party.document ?? "—"}</td>
                  <td>{party.email ?? "—"}</td>
                  <td>
                    {[party.is_customer && "Cliente", party.is_supplier && "Fornecedor"]
                      .filter(Boolean)
                      .join(" · ")}
                  </td>
                </motion.tr>
              ))}
            </tbody>
          </table>
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
    </>
  );
}

// A freshly added row flashes green and fades back to transparent.
function highlightColor(party: Party, highlightedId: string | null, alpha: 0 | 1): string {
  return `rgba(47, 178, 110, ${party.id === highlightedId ? alpha * 0.25 : 0})`;
}
