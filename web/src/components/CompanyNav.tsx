import { useEffect, useRef, useState, type FocusEvent, type PointerEvent } from "react";
import { Link, NavLink, useLocation, type Location } from "react-router-dom";
import { entryKindOf } from "../lib/entryKind";
import type { AnimatedIcon } from "./Button";
import { BoxIcon } from "./icons/box";
import { BriefcaseBusinessIcon } from "./icons/briefcase-business";
import { BadgeAlertIcon } from "./icons/badge-alert";
import { CalendarDaysIcon } from "./icons/calendar-days";
import { ChartLineIcon } from "./icons/chart-line";
import { CheckIcon } from "./icons/check";
import { ChevronDownIcon } from "./icons/chevron-down";
import { ChevronLeftIcon } from "./icons/chevron-left";
import { ChevronRightIcon } from "./icons/chevron-right";
import { CircleDollarSignIcon } from "./icons/circle-dollar-sign";
import { CreditCardIcon } from "./icons/credit-card";
import { FileTextIcon } from "./icons/file-text";
import { FolderTreeIcon } from "./icons/folder-tree";
import { HandCoinsIcon } from "./icons/hand-coins";
import { LayersIcon } from "./icons/layers";
import { RotateCCWIcon } from "./icons/rotate-ccw";
import { SendIcon } from "./icons/send";
import { UserPlusIcon } from "./icons/user-plus";
import { UsersIcon } from "./icons/users";
import { WalletIcon } from "./icons/wallet";
import { WavesIcon } from "./icons/waves";

interface NavItem {
  path: string;
  /** Which kind of entry the item shows: the receivables and the payables are the same page with `?tipo=`. */
  kind?: "receivable" | "payable";
  label: string;
  icon: AnimatedIcon;
}

/** A fold inside an area for what is set up once and rarely visited, like the reference's registers. */
interface NavRegisters {
  label: string;
  items: NavItem[];
}

/** An area is a button on the rail: it opens a panel of items, or, when it has none, is a link itself. */
interface NavArea {
  key: string;
  label: string;
  icon: AnimatedIcon;
  /** The page of an area with nothing under it. */
  link?: NavItem;
  items?: NavItem[];
  registers?: NavRegisters;
}

const AREAS: NavArea[] = [
  { key: "home", label: "Painel", icon: ChartLineIcon, link: { path: "", label: "Painel", icon: ChartLineIcon } },
  {
    key: "commercial",
    label: "Comercial",
    icon: HandCoinsIcon,
    items: [
      { path: "/quotes", label: "Orçamentos", icon: FileTextIcon },
      { path: "/sales", label: "Vendas", icon: HandCoinsIcon },
      { path: "/pos", label: "Caixa", icon: HandCoinsIcon },
      { path: "/contracts", label: "Contratos", icon: RotateCCWIcon },
      { path: "/commissions", label: "Comissões", icon: CircleDollarSignIcon },
      { path: "/price-lists", label: "Tabelas de preço", icon: LayersIcon },
    ],
  },
  {
    key: "purchases",
    label: "Suprimentos",
    icon: BoxIcon,
    items: [
      { path: "/purchases", label: "Compras", icon: BriefcaseBusinessIcon },
      { path: "/stock", label: "Estoque", icon: BoxIcon },
      { path: "/inventory-counts", label: "Inventários", icon: LayersIcon },
      { path: "/stock-reports", label: "Relatórios de estoque", icon: FileTextIcon },
    ],
  },
  {
    key: "finance",
    label: "Financeiro",
    icon: WalletIcon,
    items: [
      { path: "/entries", kind: "receivable", label: "Contas a receber", icon: HandCoinsIcon },
      { path: "/entries", kind: "payable", label: "Contas a pagar", icon: CreditCardIcon },
      { path: "/entries", label: "Lançamentos", icon: CircleDollarSignIcon },
      { path: "/cash-flow", label: "Fluxo de caixa", icon: WavesIcon },
      { path: "/transfers", label: "Transferências", icon: SendIcon },
      { path: "/reconciliation", label: "Conciliação", icon: CheckIcon },
      { path: "/bank-connections", label: "Conexões bancárias", icon: WalletIcon },
      { path: "/recurrences", label: "Recorrências", icon: RotateCCWIcon },
      { path: "/collection", label: "Cobrança", icon: CreditCardIcon },
      { path: "/delinquency", label: "Inadimplência", icon: BadgeAlertIcon },
    ],
    registers: {
      label: "Cadastros financeiros",
      items: [
        { path: "/categories", label: "Categorias", icon: FolderTreeIcon },
        { path: "/cost-centers", label: "Centros de custo", icon: LayersIcon },
        { path: "/bank-accounts", label: "Contas bancárias", icon: WalletIcon },
        { path: "/payment-methods", label: "Formas de pagamento", icon: CreditCardIcon },
        { path: "/payment-terms", label: "Prazos de pagamento", icon: CalendarDaysIcon },
      ],
    },
  },
  {
    key: "fiscal",
    label: "Fiscal",
    icon: FileTextIcon,
    items: [
      { path: "/fiscal/documents", label: "Documentos", icon: FileTextIcon },
      { path: "/fiscal/received", label: "Notas recebidas", icon: FileTextIcon },
      { path: "/fiscal/settings", label: "Configurações fiscais", icon: LayersIcon },
    ],
  },
  {
    key: "registers",
    label: "Cadastros",
    icon: UsersIcon,
    items: [
      { path: "/parties", label: "Clientes e fornecedores", icon: UsersIcon },
      { path: "/products", label: "Produtos e serviços", icon: BoxIcon },
    ],
  },
  { key: "reports", label: "Relatórios", icon: FileTextIcon, link: { path: "/reports", label: "Relatórios", icon: FileTextIcon } },
  { key: "accounting", label: "Contador", icon: CheckIcon, link: { path: "/accounting", label: "Contador", icon: CheckIcon } },
];

const SETTINGS: NavArea = {
  key: "settings",
  label: "Configurações",
  icon: UserPlusIcon,
  items: [
    { path: "/members", label: "Membros", icon: UserPlusIcon },
    { path: "/gateway", label: "Gateway fiscal", icon: LayersIcon },
    { path: "/api-keys", label: "Chaves de API", icon: SendIcon },
    { path: "/webhooks", label: "Webhooks", icon: SendIcon },
    { path: "/connectors", label: "Conectores", icon: LayersIcon },
  ],
};

function itemsOf(area: NavArea): NavItem[] {
  return [...(area.items ?? []), ...(area.registers?.items ?? [])];
}

function isCurrent(base: string, item: NavItem, location: Location): boolean {
  const target = `${base}${item.path}`;
  const path = location.pathname.replace(/\/$/, "");
  // Three links to one page: the one that is current is told by the kind in the address.
  if (item.path === "/entries") return path === target && entryKindOf(location.search) === (item.kind ?? "");
  return item.path === "" ? path === target : path === target || path.startsWith(`${target}/`);
}

/** The area whose page is showing, so the panel opens on it. */
function areaHere(areas: NavArea[], base: string, location: Location): string | null {
  const found = areas.find((area) => [...(area.link ? [area.link] : []), ...itemsOf(area)].some((item) => isCurrent(base, item, location)));
  return found?.key ?? null;
}

function ItemLink({ base, item }: { base: string; item: NavItem }) {
  const Icon = item.icon;
  const location = useLocation();
  const current = isCurrent(base, item, location);
  const to = `${base}${item.path}${item.kind ? `?tipo=${item.kind}` : ""}`;
  return (
    <Link to={to} aria-current={current ? "page" : undefined}>
      <Icon size={18} aria-hidden />
      <span>{item.label}</span>
    </Link>
  );
}

/** The links of an area: its items, then its fold of registers (closed only by choice, so nothing hides by default). */
function AreaLinks({ base, area, foldable }: { base: string; area: NavArea; foldable: boolean }) {
  const [registersOpen, setRegistersOpen] = useState(true);
  const registers = area.registers;
  return (
    <>
      {(area.items ?? []).map((item) => (
        <ItemLink key={`${item.path}${item.kind ?? ""}`} base={base} item={item} />
      ))}
      {registers && (
        <div className="nav-registers">
          {foldable ? (
            <button type="button" className="nav-registers-toggle" aria-expanded={registersOpen} onClick={() => setRegistersOpen(!registersOpen)}>
              {registers.label}
              <ChevronDownIcon size={14} aria-hidden />
            </button>
          ) : (
            <p className="nav-registers-toggle">{registers.label}</p>
          )}
          {(!foldable || registersOpen) && registers.items.map((item) => <ItemLink key={item.path} base={base} item={item} />)}
        </div>
      )}
    </>
  );
}

/**
 * The company's sections in two levels, like the reference: a rail of areas and, beside it, the panel
 * of the area being looked at. Shrunk (`collapsed`) the panel is gone and an area shows its items in a
 * flyout; on a phone the same two levels are two screens of the drawer, `level` says which.
 */
export function CompanyNav({
  companyId,
  canSeeMembers,
  collapsed,
  onExpand,
}: {
  companyId: string;
  /** The role reaches who belongs to the company: the others are not shown the link. */
  canSeeMembers: boolean;
  collapsed: boolean;
  /** Asked when an area is chosen on the shrunk rail: the panel comes back for it. */
  onExpand: () => void;
}) {
  const base = `/companies/${companyId}`;
  const location = useLocation();
  const areas = canSeeMembers ? [...AREAS, SETTINGS] : AREAS;
  const here = areaHere(areas, base, location);
  const placeKey = `${location.pathname}${location.search}`;
  const hasItems = (key: string | null) => areas.find((area) => area.key === key)?.items !== undefined;

  // Going to a page puts the panel back on its area, and a phone back on the screen of its items.
  const [seenPlace, setSeenPlace] = useState(placeKey);
  const [browsing, setBrowsing] = useState<string | null>(here);
  const [level, setLevel] = useState<"areas" | "items">(hasItems(here) ? "items" : "areas");
  if (seenPlace !== placeKey) {
    setSeenPlace(placeKey);
    setBrowsing(here);
    setLevel(hasItems(here) ? "items" : "areas");
  }

  const open = areas.find((area) => area.key === browsing && area.items !== undefined);

  // On a phone the screen a button is on goes away when the other one shows: the focus follows, or a
  // keyboard would start again from the top of the page.
  const shell = useRef<HTMLDivElement>(null);
  const focusAfter = useRef<"panel" | "rail" | null>(null);
  useEffect(() => {
    const element = shell.current;
    if (focusAfter.current === "panel") element?.querySelector<HTMLElement>(".panel-back")?.focus();
    // The area just left, else the one the page is in: `querySelector` with a list takes the first in the page.
    if (focusAfter.current === "rail") (element?.querySelector<HTMLElement>(".rail-item[data-open]") ?? element?.querySelector<HTMLElement>(".rail-item"))?.focus();
    focusAfter.current = null;
  }, [level]);

  function choose(area: NavArea) {
    setBrowsing(area.key);
    setLevel("items");
    focusAfter.current = "panel";
    onExpand();
  }

  function goBack() {
    setLevel("areas");
    focusAfter.current = "rail";
  }

  // A flyout is fixed (the rail scrolls and would clip it), so it is placed from its button when it is about
  // to show and never runs past the bottom of the window.
  function placeFlyout(event: PointerEvent<HTMLElement> | FocusEvent<HTMLElement>) {
    const slot = event.currentTarget;
    const top = Math.max(8, slot.getBoundingClientRect().top - 8);
    slot.querySelector<HTMLElement>(".flyout")?.style.setProperty("--flyout-top", `${top}px`);
  }

  return (
    <div className="nav-shell" data-level={level} ref={shell}>
      <nav className="rail" aria-label="Áreas da empresa">
        {areas.map((area) => {
          const Icon = area.icon;
          if (area.link) {
            return (
              <NavLink key={area.key} to={`${base}${area.link.path}`} end={area.link.path === ""} className="rail-item">
                <Icon size={22} aria-hidden />
                <span>{area.label}</span>
              </NavLink>
            );
          }
          return (
            <div
              key={area.key}
              className="rail-slot"
              onPointerEnter={placeFlyout}
              onFocus={placeFlyout}
              // A flyout that shows only because it has the focus can be put away with Escape (WCAG 1.4.13).
              onKeyDown={(event) => event.key === "Escape" && collapsed && (event.target as HTMLElement).blur()}
            >
              <button
                type="button"
                className="rail-item"
                aria-expanded={!collapsed && open?.key === area.key}
                aria-controls={open?.key === area.key ? "area-panel" : undefined}
                data-open={open?.key === area.key || undefined}
                data-here={here === area.key || undefined}
                onClick={() => choose(area)}
              >
                <Icon size={22} aria-hidden />
                <span>{area.label}</span>
                <ChevronRightIcon className="rail-chevron" size={16} aria-hidden />
              </button>
              {collapsed && (
                <div className="flyout" role="group" aria-label={area.label}>
                  <p className="flyout-title">{area.label}</p>
                  <AreaLinks base={base} area={area} foldable={false} />
                </div>
              )}
            </div>
          );
        })}
      </nav>
      {open && (
        <nav className="panel" id="area-panel" aria-label={open.label}>
          <div className="panel-head">
            <button type="button" className="panel-back" aria-label="Voltar às áreas" onClick={goBack}>
              <ChevronLeftIcon size={20} aria-hidden />
            </button>
            <h2 className="panel-title">{open.label}</h2>
          </div>
          <AreaLinks base={base} area={open} foldable />
        </nav>
      )}
    </div>
  );
}
