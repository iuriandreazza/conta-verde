import { useEffect } from "react";

const APP_NAME = "conta-verde";

/** The tab and the history say where the person is: the title follows the page, and goes back when it leaves. */
export function usePageTitle(title: string) {
  useEffect(() => {
    document.title = `${title} · ${APP_NAME}`;
    return () => {
      document.title = APP_NAME;
    };
  }, [title]);
}
