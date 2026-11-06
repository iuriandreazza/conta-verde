import type { ReactNode } from "react";
import { usePageTitle } from "../lib/usePageTitle";

interface PageHeaderProps {
  title: string;
  description?: string;
  actions?: ReactNode;
}

export function PageHeader({ title, description, actions }: PageHeaderProps) {
  usePageTitle(title);
  return (
    <header className="page-header">
      <div>
        <h1>{title}</h1>
        {description && <p className="page-lead">{description}</p>}
      </div>
      {actions && <div className="page-actions">{actions}</div>}
    </header>
  );
}
