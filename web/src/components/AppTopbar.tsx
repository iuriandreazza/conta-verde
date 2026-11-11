import { useEffect, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { Button } from "./Button";
import { BoxIcon } from "./icons/box";
import { BriefcaseBusinessIcon } from "./icons/briefcase-business";
import { CalendarDaysIcon } from "./icons/calendar-days";
import { CreditCardIcon } from "./icons/credit-card";
import { FolderTreeIcon } from "./icons/folder-tree";
import { LayersIcon } from "./icons/layers";
import { CircleDollarSignIcon } from "./icons/circle-dollar-sign";
import { FileTextIcon } from "./icons/file-text";
import { HandCoinsIcon } from "./icons/hand-coins";
import { LogoutIcon } from "./icons/logout";
import { PlusIcon } from "./icons/plus";
import { RotateCCWIcon } from "./icons/rotate-ccw";
import { SendIcon } from "./icons/send";
import { UserPlusIcon } from "./icons/user-plus";
import { UsersIcon } from "./icons/users";
import { WalletIcon } from "./icons/wallet";
import { MenuButton, type MenuItem } from "./MenuButton";
import type { Area, Company } from "../api";
import { useCreation, type CreateKind, type CreateOptions } from "../creation/CreationProvider";
import { msg, useT } from "../i18n";
import { LanguageSwitch } from "./LanguageSwitch";

interface AppTopbarProps {
  userName: string;
  permissions: Company["permissions"] | undefined;
  companyId: string | undefined;
  signingOut: boolean;
  signOutFailed: boolean;
  /** The company being worked in, told here because the panel beside the menu may be put away. */
  companyName?: string;
  canSwitchCompany: boolean;
  navOpen: boolean;
  onToggleNav: () => void;
  onSignOut: () => void;
}

/**
 * The letters that, with Alt, start a record from anywhere. Some browsers use a few of them for their own
 * menus, and on a Mac Option plus a letter types a symbol: the shortcut wins, in a field too, as in the
 * products people come from.
 */
const SHORTCUTS: Partial<Record<string, string>> = {
  revenue: "R",
  expense: "D",
  transfer: "T",
  quote: "O",
  sale: "V",
  party: "C",
  category: "K",
  invite: "M",
  bankAccount: "B",
  product: "P",
};

/** What a person can start from anywhere: only the registers their role may write. */
function creationItems(
  permissions: AppTopbarProps["permissions"],
  openCreate: (kind: CreateKind, options?: CreateOptions) => void,
): MenuItem[] {
  const items: (MenuItem & { area: Area; creates?: CreateKind; options?: CreateOptions })[] = [
    { key: "revenue", label: msg("New revenue"), icon: CircleDollarSignIcon, creates: "entry", options: { kind: "receivable" }, area: "finance" },
    { key: "expense", label: msg("New expense"), icon: CircleDollarSignIcon, creates: "entry", options: { kind: "payable" }, area: "finance" },
    { key: "transfer", label: msg("New transfer"), icon: SendIcon, creates: "transfer", area: "finance" },
    { key: "quote", label: msg("New quote"), icon: FileTextIcon, creates: "quote", area: "sales" },
    { key: "sale", label: msg("New order"), icon: HandCoinsIcon, creates: "sale", area: "sales" },
    { key: "purchase", label: msg("New purchase"), icon: BriefcaseBusinessIcon, creates: "purchase", area: "purchases" },
    { key: "purchaseImport", label: msg("NF-e entry (XML)"), icon: FileTextIcon, creates: "purchaseImport", area: "purchases" },
    { key: "stockMovement", label: msg("New stock movement"), icon: BoxIcon, creates: "stockMovement", area: "purchases" },
    { key: "inventoryCount", label: msg("New inventory count"), icon: LayersIcon, creates: "inventoryCount", area: "purchases" },
    { key: "party", label: msg("New customer or supplier"), icon: UsersIcon, creates: "party", area: "parties" },
    { key: "contract", label: msg("New contract"), icon: RotateCCWIcon, creates: "contract", area: "sales" },
    { key: "commissionRule", label: msg("New commission rule"), icon: HandCoinsIcon, creates: "commissionRule", area: "sales" },
    { key: "priceList", label: msg("New price list"), icon: LayersIcon, creates: "priceList", area: "sales" },
    { key: "category", label: msg("New category"), icon: FolderTreeIcon, creates: "category", area: "finance" },
    { key: "recurrence", label: msg("New recurrence"), icon: RotateCCWIcon, creates: "recurrence", area: "finance" },
    { key: "invite", label: msg("Invite someone"), icon: UserPlusIcon, creates: "invite", area: "members" },
    { key: "bankAccount", label: msg("New bank account"), icon: WalletIcon, creates: "bankAccount", area: "finance" },
    { key: "costCenter", label: msg("New cost center"), icon: LayersIcon, creates: "costCenter", area: "finance" },
    { key: "paymentMethod", label: msg("New payment method"), icon: CreditCardIcon, creates: "paymentMethod", area: "finance" },
    { key: "paymentTerm", label: msg("New payment term"), icon: CalendarDaysIcon, creates: "paymentTerm", area: "finance" },
    { key: "product", label: msg("New product or service"), icon: BoxIcon, creates: "product", area: "products" },
  ];
  return items
    .filter((item) => permissions?.[item.area] === "write")
    // A register with a sheet opens it over the page that is showing; the others still go to their page.
    .map(({ creates, options, area: _area, ...item }) => ({
      ...item,
      shortcut: SHORTCUTS[item.key],
      ...(creates ? { onSelect: () => openCreate(creates, options) } : {}),
    }));
}

/** The bar over every signed-in page: the menu on a phone, "new" from anywhere, who is signed in and how to leave. */
export function AppTopbar({
  userName,
  permissions,
  companyId,
  signingOut,
  signOutFailed,
  companyName,
  canSwitchCompany,
  navOpen,
  onToggleNav,
  onSignOut,
}: AppTopbarProps) {
  const { openCreate } = useCreation();
  const t = useT();
  const creating = companyId ? creationItems(permissions, openCreate).map((item) => ({ ...item, label: t(item.label) })) : [];
  const navigate = useNavigate();
  // A new key for the menu closes it: a shortcut used while it is open must not leave it behind the sheet.
  const [menuKey, setMenuKey] = useState(0);

  // Alt plus the letter starts the record from anywhere, unless a sheet is already open. The physical
  // key is read (`code`), since Alt changes the character some layouts type.
  useEffect(() => {
    function start(event: KeyboardEvent) {
      if (event.repeat || !event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
      const item = creating.find((candidate) => candidate.shortcut && `Key${candidate.shortcut}` === event.code);
      if (!item || document.querySelector('[role="dialog"]')) return;
      event.preventDefault();
      setMenuKey((key) => key + 1);
      if (item.onSelect) item.onSelect();
      else if (item.to) navigate(item.to);
    }
    window.addEventListener("keydown", start);
    return () => window.removeEventListener("keydown", start);
  });
  return (
    <div className="topbar">
      {companyId && (
        <button
          type="button"
          className="topbar-menu"
          aria-expanded={navOpen}
          aria-controls="app-sidebar"
          onClick={onToggleNav}
        >
          {t("Menu")}
        </button>
      )}
      {companyName && (
        <div className="topbar-company">
          <strong>{companyName}</strong>
          {canSwitchCompany && <Link to="/">{t("Switch")}</Link>}
        </div>
      )}
      <div className="topbar-actions">
        {creating.length > 0 && <MenuButton key={menuKey} variant="primary" icon={PlusIcon} label={t("New record")} items={creating} align="end" />}
        <Link to="/offline" className="topbar-link">
          {t("Offline use")}
        </Link>
        <LanguageSwitch />
        <span className="avatar" aria-hidden>
          {userName.trim().charAt(0).toUpperCase()}
        </span>
        <span className="user-name">{userName}</span>
        <Button variant="ghost" icon={LogoutIcon} pending={signingOut} onClick={onSignOut}>
          {t("Sign out")}
        </Button>
        {signOutFailed && <span role="alert">{t("Could not sign out, try again.")}</span>}
      </div>
    </div>
  );
}
