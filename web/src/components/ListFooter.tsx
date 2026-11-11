import { useT } from "../i18n";
import { Button } from "./Button";
import { ChevronDownIcon } from "./icons/chevron-down";

interface ListFooterProps {
  count: number;
  /** What the rows are, singular and plural, already in the person's language: "entry", "entries". */
  noun: [string, string];
  hasMore: boolean;
  loadingMore: boolean;
  /** The rows are being replaced: what the footer says (is there more?) is not known until they arrive. */
  refreshing?: boolean;
  onLoadMore: () => void;
}

/** Under a list: how many rows are on screen, and the way to bring more when the server has them. */
export function ListFooter({ count, noun, hasMore, loadingMore, refreshing = false, onLoadMore }: ListFooterProps) {
  const t = useT();
  const what = count === 1 ? noun[0] : noun[1];
  return (
    <div className="list-footer" aria-hidden={refreshing || undefined} data-refreshing={refreshing || undefined}>
      <p className="muted" role="status">
        {hasMore ? t("Showing {count} {noun}, and there are more", { count, noun: what }) : t("Showing {count} {noun}", { count, noun: what })}
      </p>
      {hasMore && (
        <Button type="button" variant="ghost" icon={ChevronDownIcon} pending={loadingMore} onClick={onLoadMore}>
          {t("Load more")}
        </Button>
      )}
    </div>
  );
}
