import type { Category } from "../api";

export interface Row {
  category: Category;
  /** 0 for a top-level category. */
  depth: number;
  parent: Category | null;
}

/**
 * The flat list as the tree it describes, depth first, siblings in the order the API sent them.
 * A category whose parent is not in the list (the list was narrowed) is shown at the top.
 */
export function treeRows(categories: Category[]): Row[] {
  const byId = new Map(categories.map((category) => [category.id, category]));
  const children = new Map<string | null, Category[]>();
  for (const category of categories) {
    const key = category.parent_id !== null && byId.has(category.parent_id) ? category.parent_id : null;
    children.set(key, [...(children.get(key) ?? []), category]);
  }
  const rows: Row[] = [];
  const visit = (parentId: string | null, depth: number) => {
    for (const category of children.get(parentId) ?? []) {
      rows.push({ category, depth, parent: parentId === null ? null : (byId.get(parentId) ?? null) });
      visit(category.id, depth + 1);
    }
  };
  visit(null, 0);
  return rows;
}
