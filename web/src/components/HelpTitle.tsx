import type { ReactNode } from "react";
import { useT } from "../i18n";
import { InfoTip } from "./InfoTip";

/** A heading with its explanation behind an (i) on the same line, so the text is not a paragraph under it. */
export function HelpTitle({
  level = 3,
  id,
  title,
  children,
}: {
  level?: 2 | 3;
  id?: string;
  title: string;
  children: ReactNode;
}) {
  const t = useT();
  const Heading = level === 2 ? "h2" : "h3";
  return (
    <div className="title-with-tip">
      <Heading id={id}>{title}</Heading>
      <InfoTip label={t("About {title}", { title })} align="start">
        {children}
      </InfoTip>
    </div>
  );
}
