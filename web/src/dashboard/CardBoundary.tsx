import { Component, type ReactNode } from "react";
import { Notice } from "../components/Notice";
import { useT } from "../i18n";

/** A class cannot use a hook, so the words of the failure are said by a component of their own. */
function Failed({ title }: { title: string }) {
  const t = useT();
  return <Notice message={t("Could not show the card “{title}”.", { title })} />;
}

/**
 * One card of the dashboard that cannot be drawn (a reply that is not what it expects) says so in its place
 * and the rest of the page goes on: the home page must never go blank because one figure failed.
 */
export class CardBoundary extends Component<{ title: string; children: ReactNode }, { failed: boolean }> {
  state = { failed: false };

  static getDerivedStateFromError() {
    return { failed: true };
  }

  render() {
    return this.state.failed ? <Failed title={this.props.title} /> : this.props.children;
  }
}
