import { Component, type ReactNode } from "react";
import { Notice } from "../components/Notice";

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
    return this.state.failed ? <Notice message={`Não foi possível mostrar o quadro “${this.props.title}”.`} /> : this.props.children;
  }
}
