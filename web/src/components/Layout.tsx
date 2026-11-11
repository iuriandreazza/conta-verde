import { motion } from "motion/react";
import { useEffect, useRef, useState, type MouseEvent } from "react";
import { Link, Outlet, useLocation, useMatch } from "react-router-dom";
import { useAuth } from "../auth";
import { AppTopbar } from "./AppTopbar";
import { EmailConfirmationBanner } from "./EmailConfirmationBanner";
import { OfflineBanner } from "./OfflineBanner";
import { CreationProvider } from "../creation/CreationProvider";
import { CompanyNav } from "./CompanyNav";
import { Footer } from "./Footer";
import { ChevronLeftIcon } from "./icons/chevron-left";
import { ChevronRightIcon } from "./icons/chevron-right";
import { XIcon } from "./icons/x";
import { Logo } from "./Logo";
import { useT } from "../i18n";

const COLLAPSED_KEY = "conta-verde.sidebar-collapsed";

// Storage can be missing or refuse (a private window): the menu then just starts open.
function readCollapsed(): boolean {
  try {
    return window.localStorage.getItem(COLLAPSED_KEY) === "true";
  } catch {
    return false;
  }
}

export function Layout() {
  const { session, signOut } = useAuth();
  const t = useT();
  const [signOutFailed, setSignOutFailed] = useState(false);
  const [signingOut, setSigningOut] = useState(false);
  const [navOpen, setNavOpen] = useState(false);
  // On a wide screen the panel beside the rail can be put away; the phone's drawer ignores it.
  const [collapsed, setCollapsed] = useState(readCollapsed);
  const location = useLocation();
  const mainRef = useRef<HTMLElement>(null);
  // The drawer on a phone covers the page: going somewhere closes it.
  useEffect(() => setNavOpen(false), [location.pathname, location.search]);
  // The drawer takes the focus when it opens, and gives it back to its button when it closes.
  useEffect(() => {
    if (!navOpen) return;
    const shell = document.querySelector<HTMLElement>("#app-sidebar .nav-shell");
    // The first thing of the screen the drawer opens on: the way back when it shows an area, else the first area.
    const first = shell?.dataset.level === "items" ? ".panel-back" : ".rail a, .rail button";
    shell?.querySelector<HTMLElement>(first)?.focus();
  }, [navOpen]);

  function changeCollapsed(next: boolean) {
    setCollapsed(next);
    try {
      window.localStorage.setItem(COLLAPSED_KEY, String(next));
    } catch {
      // Not remembered, still collapsed for now.
    }
  }

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
  const onPublicPage =
    location.pathname === "/login" ||
    location.pathname === "/register" ||
    location.pathname === "/forgot-password" ||
    location.pathname.startsWith("/reset-password/") ||
    location.pathname.startsWith("/confirm-email/");
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
      <CreationProvider companyId={undefined}>
      <div className="auth-shell">
        <aside className="auth-aside">
          <Link to="/" className="brand" aria-label={t("conta-verde, start")}>
            <Logo size={34} animated />
          </Link>
          <div className="auth-pitch">
            <p className="auth-pitch-title">{t("Simple management for your business to grow.")}</p>
            <ul>
              <li>{t("Customers, suppliers, products and services in one place.")}</li>
              <li>{t("Every change recorded: who made it, what changed and when.")}</li>
              <li>{t("Free software: the code is yours to read and check.")}</li>
            </ul>
          </div>
        </aside>
        <div className="auth-main">
          {page}
          <Footer />
        </div>
      </div>
      </CreationProvider>
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
    // The provider is in both shells, at the same place: the page keeps its state when the session arrives.
    <CreationProvider companyId={companyId}>
    <div className="app" data-collapsed={collapsed || undefined}>
      {/* Inside the sidebar, not before it: the sign-in layout and this one put <main> at the same position,
          which keeps the sign-up page mounted (and its outcome) when the session arrives. */}
      <aside
        className="sidebar"
        id="app-sidebar"
        data-open={navOpen || undefined}
        onKeyDown={(event) => event.key === "Escape" && navOpen && closeNav()}
      >
        <a href="#main" className="skip-link" onClick={skipToContent}>
          {t("Skip to the content")}
        </a>
        {companyId && (
          <button type="button" className="sidebar-close" aria-label={t("Close the menu")} onClick={closeNav}>
            <XIcon size={22} aria-hidden />
          </button>
        )}
        <div className="sidebar-head">
          <Link to="/" className="brand" aria-label={t("conta-verde, start")}>
            <Logo size={30} />
          </Link>
        </div>
        {company && (
          <div className="company-switch">
            <span className="company-switch-label">{t("Company")}</span>
            <strong className="company-switch-name">{company.name}</strong>
            {companies.length > 1 && <Link to="/">{t("Switch company")}</Link>}
          </div>
        )}
        {companyId && (
          <CompanyNav
            companyId={companyId}
            collapsed={collapsed}
            // Choosing an area on the phone's drawer must not undo what was chosen for the wide menu.
            onExpand={() => collapsed && window.matchMedia?.("(min-width: 52.0625rem)").matches !== false && changeCollapsed(false)}
            canSeeMembers={company?.permissions.members !== undefined}
          />
        )}
        {companyId && (
          <button
            type="button"
            className="sidebar-toggle"
            aria-expanded={!collapsed}
            aria-label={collapsed ? t("Expand the menu") : t("Collapse the menu")}
            title={collapsed ? t("Expand the menu") : t("Collapse the menu")}
            onClick={() => changeCollapsed(!collapsed)}
          >
            {collapsed ? <ChevronRightIcon size={18} aria-hidden /> : <ChevronLeftIcon size={18} aria-hidden />}
          </button>
        )}
      </aside>
      <div className="app-main">
        <AppTopbar
          userName={userName}
          permissions={company?.permissions}
          companyId={companyId}
          companyName={company?.name}
          canSwitchCompany={companies.length > 1}
          signingOut={signingOut}
          signOutFailed={signOutFailed}
          navOpen={navOpen}
          onToggleNav={() => (navOpen ? closeNav() : setNavOpen(true))}
          onSignOut={handleSignOut}
        />
        <OfflineBanner />
        {!session.profile.email_verified && <EmailConfirmationBanner email={session.profile.user.email} />}
        {page}
        <Footer />
      </div>
      {navOpen && <button type="button" className="drawer-scrim" aria-hidden tabIndex={-1} onClick={closeNav} />}
    </div>
    </CreationProvider>
  );
}
