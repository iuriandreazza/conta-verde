import { useCallback, useEffect, useRef, useState, type FormEvent } from "react";
import { useParams } from "react-router-dom";
import { api, errorMessage, type Party } from "../api";
import { useAuth } from "../auth";

export function PartiesPage() {
  const { companyId = "" } = useParams();
  const { session } = useAuth();
  const [parties, setParties] = useState<Party[]>([]);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [formError, setFormError] = useState<string | null>(null);

  // A slow reply for a company the user has already left must not overwrite the current list.
  const currentCompany = useRef(companyId);
  currentCompany.current = companyId;

  const companyName =
    session.status === "authenticated"
      ? session.profile.companies.find((company) => company.id === companyId)?.name
      : undefined;

  const loadFirstPage = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const page = await api.listParties(companyId);
      if (currentCompany.current !== companyId) return;
      setParties(page.items);
      setNextCursor(page.next_cursor);
    } catch (failure) {
      if (currentCompany.current === companyId) setError(errorMessage(failure));
    } finally {
      if (currentCompany.current === companyId) setLoading(false);
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
      if (currentCompany.current !== companyId) return;
      setParties((current) => [...current, ...page.items]);
      setNextCursor(page.next_cursor);
    } catch (failure) {
      if (currentCompany.current === companyId) setError(errorMessage(failure));
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
      await api.createParty(companyId, {
        name: String(form.get("name") ?? ""),
        document: optional("document"),
        email: optional("email"),
        is_customer: form.get("is_customer") === "on",
        is_supplier: form.get("is_supplier") === "on",
      });
      formElement.reset();
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
          {formError && <p role="alert" className="error">{formError}</p>}
          <button type="submit" disabled={creating}>
            Adicionar
          </button>
        </form>
      </section>

      <section className="card">
        <h2>Cadastrados</h2>
        {error && <p role="alert" className="error">{error}</p>}
        {loading ? (
          <p role="status">Carregando…</p>
        ) : parties.length === 0 && !error ? (
          <p>Nenhum cadastro ainda.</p>
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
              {parties.map((party) => (
                <tr key={party.id}>
                  <td>{party.name}</td>
                  <td>{party.document ?? "—"}</td>
                  <td>{party.email ?? "—"}</td>
                  <td>
                    {[party.is_customer && "Cliente", party.is_supplier && "Fornecedor"]
                      .filter(Boolean)
                      .join(" · ")}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        {nextCursor !== null && (
          <button type="button" disabled={loadingMore} onClick={() => void loadMore()}>
            Carregar mais
          </button>
        )}
      </section>
    </>
  );
}
