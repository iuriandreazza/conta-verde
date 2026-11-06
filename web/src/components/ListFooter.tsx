import { Button } from "./Button";
import { ChevronDownIcon } from "./icons/chevron-down";

interface ListFooterProps {
  count: number;
  /** What the rows are, singular and plural: "lançamento", "lançamentos". */
  noun: [string, string];
  hasMore: boolean;
  loadingMore: boolean;
  onLoadMore: () => void;
}

/** Under a list: how many rows are on screen, and the way to bring more when the server has them. */
export function ListFooter({ count, noun, hasMore, loadingMore, onLoadMore }: ListFooterProps) {
  return (
    <div className="list-footer">
      <p className="muted" role="status">
        Mostrando {count} {count === 1 ? noun[0] : noun[1]}
        {hasMore && ", e há mais"}
      </p>
      {hasMore && (
        <Button type="button" variant="ghost" icon={ChevronDownIcon} pending={loadingMore} onClick={onLoadMore}>
          Carregar mais
        </Button>
      )}
    </div>
  );
}
