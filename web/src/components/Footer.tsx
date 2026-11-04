// AGPL section 13: users of a modified network service must be offered its source. Forks set
// VITE_SOURCE_URL at build time so the link points at the code they actually run.
export const SOURCE_URL: string =
  import.meta.env.VITE_SOURCE_URL ?? "https://github.com/iuriandreazza/conta-verde";

export function Footer() {
  return (
    <footer className="footer">
      <span>conta-verde é software livre sob a GNU AGPL v3 ou posterior.</span>{" "}
      <a href={SOURCE_URL} target="_blank" rel="noreferrer">
        Código-fonte
      </a>
    </footer>
  );
}
