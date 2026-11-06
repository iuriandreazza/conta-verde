import type { DocumentLine, PurchaseReceipt } from "../api";
import { quantityToApi, toQuantity } from "./saleLines";

/** What each line of an order has received so far (the API's quantities, in thousandths), summed over its receipts. */
export function receivedByLine(lines: DocumentLine[], receipts: PurchaseReceipt[]): bigint[] {
  const received = lines.map(() => 0n);
  for (const receipt of receipts) {
    for (const item of receipt.items) {
      const quantity = toQuantity(item.quantity);
      if (quantity !== null && item.line >= 1 && item.line <= received.length) {
        received[item.line - 1] += quantity;
      }
    }
  }
  return received;
}

/** What is still to arrive on each line, never below zero, as the API's decimal text. */
export function leftByLine(lines: DocumentLine[], receipts: PurchaseReceipt[]): string[] {
  const received = receivedByLine(lines, receipts);
  return lines.map((line, index) => {
    const left = (toQuantity(line.quantity) ?? 0n) - received[index];
    return quantityToApi(left > 0n ? left : 0n);
  });
}
