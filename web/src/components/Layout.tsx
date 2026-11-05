import { motion } from "motion/react";
import { useState } from "react";
import { Link, Outlet, useLocation, useMatch } from "react-router-dom";
import { useAuth } from "../auth";
import { Button } from "./Button";
import { CompanyNav } from "./CompanyNav";
import { Footer } from "./Footer";
import { LogoutIcon } from "./icons/logout";
import { Logo } from "./Logo";

export function Layout() {
  const { session, signOut } = useAuth();
  const [signOutFailed, setSignOutFailed] = useState(false);
  const [signingOut, setSigningOut] = useState(false);
  const location = useLocation();
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

  const userName = session.profile.user.name;
  return (
    <div className="app">
      <aside className="sidebar">
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
        {companyId && <CompanyNav companyId={companyId} />}
        <div className="sidebar-user">
          <span className="avatar" aria-hidden>
            {userName.trim().charAt(0).toUpperCase()}
          </span>
          <span className="user-name">{userName}</span>
          <Button variant="ghost" icon={LogoutIcon} pending={signingOut} onClick={handleSignOut}>
            Sair
          </Button>
          {signOutFailed && <span role="alert">Não foi possível sair, tente novamente.</span>}
        </div>
      </aside>
      <div className="app-main">
        {page}
        <Footer />
      </div>
    </div>
  );
}
