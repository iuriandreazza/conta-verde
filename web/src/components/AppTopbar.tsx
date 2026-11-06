import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Button } from "./Button";
import { BoxIcon } from "./icons/box";
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
import { useCreation, type CreateKind } from "../creation/CreationProvider";

interface AppTopbarProps {
  userName: string;
  permissions: Company["permissions"] | undefined;
  companyId: string | undefined;
  signingOut: boolean;
  signOutFailed: boolean;
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
  entry: "L",
  transfer: "T",
  quote: "O",
  sale: "V",
  party: "C",
  category: "K",
  recurrence: "R",
  invite: "M",
  bankAccount: "B",
  product: "P",
};

/** What a person can start from anywhere: only the registers their role may write. */
function creationItems(
  companyId: string,
  permissions: AppTopbarProps["permissions"],
  openCreate: (kind: CreateKind) => void,
): MenuItem[] {
  const base = `/companies/${companyId}`;
  const items: (MenuItem & { area: Area; creates?: CreateKind })[] = [
    { key: "entry", label: "Novo lançamento", icon: CircleDollarSignIcon, creates: "entry", area: "finance" },
    { key: "transfer", label: "Nova transferência", icon: SendIcon, creates: "transfer", area: "finance" },
    { key: "quote", label: "Novo orçamento", icon: FileTextIcon, to: `${base}/quotes`, area: "sales" },
    { key: "sale", label: "Nova venda", icon: HandCoinsIcon, to: `${base}/sales`, area: "sales" },
    { key: "party", label: "Novo cliente ou fornecedor", icon: UsersIcon, creates: "party", area: "parties" },
    { key: "contract", label: "Novo contrato", icon: RotateCCWIcon, creates: "contract", area: "sales" },
    { key: "commissionRule", label: "Nova regra de comissão", icon: HandCoinsIcon, creates: "commissionRule", area: "sales" },
    { key: "priceList", label: "Nova tabela de preço", icon: LayersIcon, creates: "priceList", area: "sales" },
    { key: "category", label: "Nova categoria", icon: FolderTreeIcon, creates: "category", area: "finance" },
    { key: "recurrence", label: "Nova recorrência", icon: RotateCCWIcon, creates: "recurrence", area: "finance" },
    { key: "invite", label: "Convidar alguém", icon: UserPlusIcon, creates: "invite", area: "members" },
    { key: "bankAccount", label: "Nova conta bancária", icon: WalletIcon, creates: "bankAccount", area: "finance" },
    { key: "costCenter", label: "Novo centro de custo", icon: LayersIcon, creates: "costCenter", area: "finance" },
    { key: "paymentMethod", label: "Nova forma de pagamento", icon: CreditCardIcon, creates: "paymentMethod", area: "finance" },
    { key: "paymentTerm", label: "Novo prazo de pagamento", icon: CalendarDaysIcon, creates: "paymentTerm", area: "finance" },
    { key: "product", label: "Novo produto ou serviço", icon: BoxIcon, creates: "product", area: "products" },
  ];
  return items
    .filter((item) => permissions?.[item.area] === "write")
    // A register with a sheet opens it over the page that is showing; the others still go to their page.
    .map(({ creates, area: _area, ...item }) => ({
      ...item,
      shortcut: SHORTCUTS[item.key],
      ...(creates ? { onSelect: () => openCreate(creates) } : {}),
    }));
}

/** The bar over every signed-in page: the menu on a phone, "new" from anywhere, who is signed in and how to leave. */
export function AppTopbar({
  userName,
  permissions,
  companyId,
  signingOut,
  signOutFailed,
  navOpen,
  onToggleNav,
  onSignOut,
}: AppTopbarProps) {
  const { openCreate } = useCreation();
  const creating = companyId ? creationItems(companyId, permissions, openCreate) : [];
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
          Menu
        </button>
      )}
      <div className="topbar-actions">
        {creating.length > 0 && <MenuButton key={menuKey} variant="primary" icon={PlusIcon} label="Novo registro" items={creating} align="end" />}
        <span className="avatar" aria-hidden>
          {userName.trim().charAt(0).toUpperCase()}
        </span>
        <span className="user-name">{userName}</span>
        <Button variant="ghost" icon={LogoutIcon} pending={signingOut} onClick={onSignOut}>
          Sair
        </Button>
        {signOutFailed && <span role="alert">Não foi possível sair, tente novamente.</span>}
      </div>
    </div>
  );
}
