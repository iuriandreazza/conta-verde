import { Link } from "react-router-dom";

/** What an address the app has no page for shows, so a mistyped or mangled link is never a blank screen. */
export function NotFoundPage() {
  return (
    <section className="card narrow">
      <h1>Página não encontrada</h1>
      <p>Este endereço não leva a nenhuma página. Confira o link que você recebeu.</p>
      <p>
        <Link to="/">Voltar ao início</Link>
      </p>
    </section>
  );
}
