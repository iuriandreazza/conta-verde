import { useOfflineStatus } from "../pwa/status";

const time = new Intl.DateTimeFormat("pt-BR", { dateStyle: "short", timeStyle: "short" });

/** Said whenever the screen may be showing the device's copy: what the person sees is old, and nothing can be saved until the network is back. */
export function OfflineBanner() {
  const { offline, cachedAt } = useOfflineStatus();
  if (!offline && cachedAt === null) return null;
  return (
    <div className="banner" role="status">
      {offline ? "Sem conexão." : "Sem resposta do servidor."}{" "}
      {cachedAt !== null ? `Você vê dados guardados neste aparelho, de ${time.format(cachedAt)}. ` : "Os dados que aparecem podem estar desatualizados. "}
      Não dá para salvar alterações enquanto estiver offline.
    </div>
  );
}
