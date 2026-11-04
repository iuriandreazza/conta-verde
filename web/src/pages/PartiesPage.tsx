import { useCallback, useEffect, useState, type FormEvent } from "react";
import { useParams } from "react-router-dom";
import { ApiError, api, type Party } from "../api";
import { useAuth } from "../auth";

function describe(failure: unknown): string {
  return failure instanceof ApiError ? failure.message : "Something went wrong, try again.";
}

export function PartiesPage() {
  const { companyId = "" } = useParams();
  const { session } = useAuth();
  const [parties, setParties] = useState<Party[]>([]);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [formError, setFormError] = useState<string | null>(null);

  const companyName =
    session.status === "authenticated"
      ? session.profile.companies.find((company) => company.id === companyId)?.name
      : undefined;

  const loadFirstPage = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const page = await api.listParties(companyId);
      setParties(page.items);
      setNextCursor(page.next_cursor);
    } catch (failure) {
      setError(describe(failure));
    } finally {
      setLoading(false);
    }
  }, [companyId]);

  useEffect(() => {
    void loadFirstPage();
  }, [loadFirstPage]);

  async function loadMore() {
    if (nextCursor === null) {
      return;
    }
    try {
      const page = await api.listParties(companyId, nextCursor);
      setParties((current) => [...current, ...page.items]);
      setNextCursor(page.next_cursor);
    } catch (failure) {
      setError(describe(failure));
    }
  }

  async function handleCreate(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const formElement = event.currentTarget;
    const form = new FormData(formElement);
    const optional = (name: string) => String(form.get(name) ?? "").trim() || undefined;
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
      setFormError(describe(failure));
    }
  }

  return (
    <>
      <h1>{companyName ?? "Customers and suppliers"}</h1>

      <section className="card">
        <h2>New customer or supplier</h2>
        <form onSubmit={handleCreate}>
          <label>
            Name
            <input name="name" required />
          </label>
          <label>
            CPF or CNPJ
            <input name="document" inputMode="numeric" />
          </label>
          <label>
            Email
            <input name="email" type="email" />
          </label>
          <fieldset>
            <legend>Role</legend>
            <label className="inline">
              <input name="is_customer" type="checkbox" defaultChecked /> Customer
            </label>
            <label className="inline">
              <input name="is_supplier" type="checkbox" /> Supplier
            </label>
          </fieldset>
          {formError && <p role="alert" className="error">{formError}</p>}
          <button type="submit">Add</button>
        </form>
      </section>

      <section className="card">
        <h2>Registered</h2>
        {error && <p role="alert" className="error">{error}</p>}
        {loading ? (
          <p role="status">Loading…</p>
        ) : parties.length === 0 && !error ? (
          <p>Nobody registered yet.</p>
        ) : (
          <table>
            <thead>
              <tr>
                <th>Name</th>
                <th>Document</th>
                <th>Email</th>
                <th>Role</th>
              </tr>
            </thead>
            <tbody>
              {parties.map((party) => (
                <tr key={party.id}>
                  <td>{party.name}</td>
                  <td>{party.document ?? "—"}</td>
                  <td>{party.email ?? "—"}</td>
                  <td>
                    {[party.is_customer && "Customer", party.is_supplier && "Supplier"]
                      .filter(Boolean)
                      .join(" · ")}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        {nextCursor !== null && (
          <button type="button" onClick={() => void loadMore()}>
            Load more
          </button>
        )}
      </section>
    </>
  );
}
