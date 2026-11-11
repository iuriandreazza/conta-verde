import { useState } from "react";
import { useAuth } from "../auth";
import { Button } from "../components/Button";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { isOfflineEnabled, setOfflineEnabled } from "../pwa/client";

/** Where the person turns the offline mode on or off for this device, knowing what that leaves on it. It is their device and their call. */
export function OfflinePage() {
  const { session } = useAuth();
  const userId = session.status === "authenticated" ? session.profile.user.id : null;
  const [enabled, setEnabled] = useState(() => userId !== null && isOfflineEnabled(userId));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const supported = typeof navigator !== "undefined" && "serviceWorker" in navigator && typeof caches !== "undefined";

  async function change(on: boolean) {
    if (userId === null || busy) return;
    setBusy(true);
    setError(null);
    try {
      await setOfflineEnabled(on, userId);
      setEnabled(on);
    } catch {
      setError("Não foi possível mudar o modo offline neste aparelho.");
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <PageHeader
        title="Uso offline"
        description="O aplicativo pode ser instalado no aparelho e abrir sem internet. Ligando o modo offline, as telas que você abriu (clientes, produtos, lançamentos, vendas, orçamentos, estoque, saldos, o extrato bancário a conciliar e o painel) ficam guardadas aqui para consulta quando a conexão cair."
      />
      <section className="card narrow">
        <h2>Neste aparelho</h2>
        {!supported && <Notice message="Este navegador não permite o modo offline." />}
        <p>
          O modo offline está <strong>{enabled ? "ligado" : "desligado"}</strong>.
        </p>
        <ul>
          <li>
            Ligado, uma cópia do que você consulta fica guardada <strong>neste aparelho</strong>, inclusive dados pessoais de clientes (documento, endereço, contatos), por no
            máximo <strong>7 dias</strong> ou até você <strong>sair</strong>, o que vier primeiro. Use só em aparelho seu.
          </li>
          <li>Também ficam guardados os termos que você digitou nas buscas dessas telas, junto com o resultado.</li>
          <li>Offline você só <strong>consulta</strong>: nada é salvo sem conexão, e uma tela que não alcança o servidor avisa.</li>
          <li>Nunca ficam guardados: membros, chaves de API, webhooks, contador, fiscal, anexos, exportações e históricos.</li>
          <li>Desligar apaga a cópia na hora.</li>
        </ul>
        <Notice message={error} />
        <Button type="button" pending={busy} disabled={!supported || userId === null} onClick={() => void change(!enabled)}>
          {enabled ? "Desligar e apagar a cópia" : "Ligar o modo offline neste aparelho"}
        </Button>
      </section>
    </>
  );
}
