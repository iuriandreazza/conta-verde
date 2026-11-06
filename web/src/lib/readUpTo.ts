/** The most records of a register a form offers to point at: the first by name. */
export const MAX_CHOICES = 200;

/** Reads a list page after page, up to `cap` records, and says whether it reached the end of the list. */
export async function readCapped<T>(
  fetchPage: (after?: string) => Promise<{ items: T[]; next_cursor: string | null }>,
  cap: number,
): Promise<{ items: T[]; complete: boolean }> {
  const items: T[] = [];
  let after: string | undefined;
  while (items.length < cap) {
    const page = await fetchPage(after);
    items.push(...page.items);
    if (page.next_cursor === null) return { items: items.slice(0, cap), complete: items.length <= cap };
    after = page.next_cursor;
  }
  return { items: items.slice(0, cap), complete: false };
}

/** As `readCapped`, for a select that cannot be paged. */
export async function readUpTo<T>(
  fetchPage: (after?: string) => Promise<{ items: T[]; next_cursor: string | null }>,
  cap: number = MAX_CHOICES,
): Promise<T[]> {
  return (await readCapped(fetchPage, cap)).items;
}
