// Joins class names, skipping falsy entries. The animated icons call this with their `className`.
export function cn(...classes: Array<string | false | null | undefined>): string {
  return classes.filter(Boolean).join(" ");
}
