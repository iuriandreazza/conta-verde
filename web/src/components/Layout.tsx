import { motion } from "motion/react";
import { useEffect, useRef, useState, type MouseEvent } from "react";
import { Link, Outlet, useLocation, useMatch } from "react-router-dom";
import { useAuth } from "../auth";
import { AppTopbar } from "./AppTopbar";
import { CompanyNav } from "./CompanyNav";
import { Footer } from "./Footer";
import { Logo } from "./Logo";

export function Layout() {
  const { session, signOut } = useAuth();
  const [signOutFailed, setSignOutFailed] = useState(false);
  const [signingOut, setSigningOut] = useState(false);
  const [navOpen, setNavOpen] = useState(false);
  const location = useLocation();
  const mainRef = useRef<HTMLElement>(null);
  // The drawer on a phone covers the page: going somewhere closes it.
  useEffect(() => setNavOpen(false), [location.pathname]);
  // The drawer takes the focus when it opens, and gives it back to its button when it closes.
  useEffect(() => {
    if (navOpen) document.querySelector<HTMLElement>("#app-sidebar .side-nav a")?.focus();
  }, [navOpen]);

  function closeNav() {
    setNavOpen(false);
    document.querySelector<HTMLElement>(".topbar-menu")?.focus();
  }
  const companyId = useMatch("/companies/:companyId/*")?.params.companyId;

  // The session only ends once the server confirms it, so a failure must stay visible.
  function handleSignOut() {
    setSignOutFailed(false);
    setSigningOut(true);
    signOut()
      .catch(() => setSignOutFailed(true))
      .finally(() => setSigningOut(false));
  }

  const authenticated = session.status === "authenticated";
  const companies = authenticated ? session.profile.companies : [];
  const company = companies.find((item) => item.id === companyId);

  // Enter-only keeps this simple: no exit phase to coordinate with redirects. Keying on the path
  // also remounts the page on every navigation, which drops any state (and any reply still in
  // flight) from the previous page, e.g. the parties of the company the user just left.
  const page = (
    <motion.main
      id="main"
      ref={mainRef}
      tabIndex={-1}
      key={location.pathname}
      initial={{ opacity: 0, y: 10 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.2, ease: "easeOut" }}
    >
      <Outlet />
    </motion.main>
  );

  // Until the session is known, a protected page gets no chrome: the sign-in layout would flash for
  // every signed-in user who reloads. The sign-in and sign-up pages keep their layout, or the form
  // would be remounted (and lose what was typed) once the answer arrives.
  const onPublicPage = location.pathname === "/login" || location.pathname === "/register";
  if (session.status === "loading" && !onPublicPage) {
    return (
      <div className="app-main">
        {page}
        <Footer />
      </div>
    );
  }

  if (!authenticated) {
    return (
      <div className="auth-shell">
        <aside className="auth-aside">
          <Link to="/" className="brand" aria-label="conta-verde, início">
            <Logo size={34} animated />
          </Link>
          <div className="auth-pitch">
            <p className="auth-pitch-title">Gestão simples para o seu negócio crescer.</p>
            <ul>
              <li>Clientes, fornecedores, produtos e serviços em um só lugar.</li>
              <li>Cada alteração registrada: quem fez, o que mudou e quando.</li>
              <li>Software livre: o código é seu para ler e conferir.</li>
            </ul>
          </div>
        </aside>
        <div className="auth-main">
          {page}
          <Footer />
        </div>
      </div>
    );
  }

  // The sections come before the content in reading order; this jumps over them. A plain "#main" link
  // would also write the fragment into the URL the router owns.
  function skipToContent(event: MouseEvent) {
    event.preventDefault();
    mainRef.current?.focus();
  }

  const userName = session.profile.user.name;
  return (
    <div className="app">
      {/* Inside the sidebar, not before it: the sign-in layout and this one put <main> at the same position,
          which keeps the sign-up page mounted (and its outcome) when the session arrives. */}
      <aside
        className="sidebar"
        id="app-sidebar"
        data-open={navOpen || undefined}
        onKeyDown={(event) => event.key === "Escape" && navOpen && closeNav()}
      >
        <a href="#main" className="skip-link" onClick={skipToContent}>
          Ir para o conteúdo
        </a>
        <Link to="/" className="brand" aria-label="conta-verde, início">
          <Logo size={30} />
        </Link>
        {company && (
          <div className="company-switch">
            <span className="company-switch-label">Empresa</span>
            <strong className="company-switch-name">{company.name}</strong>
            {companies.length > 1 && <Link to="/">Trocar de empresa</Link>}
          </div>
        )}
        {companyId && (
          <CompanyNav companyId={companyId} canSeeMembers={company?.permissions.members !== undefined} />
        )}
      </aside>
      <div className="app-main">
        <AppTopbar
          userName={userName}
          permissions={company?.permissions}
          companyId={companyId}
          signingOut={signingOut}
          signOutFailed={signOutFailed}
          navOpen={navOpen}
          onToggleNav={() => (navOpen ? closeNav() : setNavOpen(true))}
          onSignOut={handleSignOut}
        />
        {page}
        <Footer />
      </div>
      {navOpen && <button type="button" className="drawer-scrim" aria-label="Fechar o menu" onClick={closeNav} />}
    </div>
  );
}
