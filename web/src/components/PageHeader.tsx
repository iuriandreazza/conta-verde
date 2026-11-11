import type { ReactNode } from "react";
import { useT } from "../i18n";
import { usePageTitle } from "../lib/usePageTitle";
import { InfoTip } from "./InfoTip";

interface PageHeaderProps {
  title: string;
  /** What the screen is for, behind an (i) beside the title. */
  description?: string;
  /** Facts about what the page shows (a customer, a charge): always visible, never behind the (i). */
  subtitle?: string;
  actions?: ReactNode;
}

export function PageHeader({ title, description, subtitle, actions }: PageHeaderProps) {
  const t = useT();
  usePageTitle(title);
  return (
    <header className="page-header">
      <div>
        <div className="title-with-tip">
          <h1>{title}</h1>
          {description && (
            <InfoTip label={t("About this screen: {title}", { title })} align="start">
              <p>{description}</p>
            </InfoTip>
          )}
        </div>
        {subtitle && <p className="page-lead">{subtitle}</p>}
      </div>
      {actions && <div className="page-actions">{actions}</div>}
    </header>
  );
}
