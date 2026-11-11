import { useOfflineStatus } from "../pwa/status";

const time = new Intl.DateTimeFormat("pt-BR", { dateStyle: "short", timeStyle: "short" });

/** Said whenever the screen may be showing the device's copy: what the person sees is old, and nothing can be saved until the network is back. */
export function OfflineBanner() {
  const { offline, cachedAt, stale } = useOfflineStatus();
  if (!offline && !stale) return null;
  return (
    <div className="banner" role="status">
      <span>
        {offline ? "Sem conexão." : "A conexão voltou."}{" "}
        {cachedAt !== null ? `O que aparece em parte é uma cópia guardada neste aparelho, de ${time.format(cachedAt)}. ` : "Os dados que aparecem podem estar desatualizados. "}
        {offline ? "Não dá para salvar alterações enquanto estiver offline." : "Atualize para ver os dados de agora."}
      </span>
      {!offline && (
        <button type="button" className="btn btn-ghost btn-small" onClick={() => window.location.reload()}>
          Atualizar
        </button>
      )}
    </div>
  );
}
