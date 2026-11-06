/** The most records of a register a form offers to point at: the first by name. */
export const MAX_CHOICES = 200;

/** Reads a list page after page, up to `cap` records: a select cannot be paged. */
export async function readUpTo<T>(
  fetchPage: (after?: string) => Promise<{ items: T[]; next_cursor: string | null }>,
  cap: number = MAX_CHOICES,
): Promise<T[]> {
  const items: T[] = [];
  let after: string | undefined;
  while (items.length < cap) {
    const page = await fetchPage(after);
    items.push(...page.items);
    if (page.next_cursor === null) break;
    after = page.next_cursor;
  }
  return items.slice(0, cap);
}
