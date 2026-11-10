import assert from "node:assert/strict";
import { test } from "node:test";
import { orderIdOf, referenceOf, saleOf } from "../src/mapping.js";
import { order, product } from "./standins.js";

const cafe = product("cafe-500");
const options = { storeId: "centro", customerId: "c0000000-0000-4000-8000-000000000001", categoryId: "c0000000-0000-4000-8000-000000000002", paymentTermId: null, productIds: new Map([["CAFE-500", cafe.id]]) };

test("an order becomes the request of a sale from the order alone, with the store in its reference", () => {
  const found = order({ id: 123, number: "123" });
  const { request, problem } = saleOf(found, options);
  assert.equal(problem, undefined);
  assert.equal(request.external_reference, "woocommerce:centro:123");
  assert.equal(request.sold_on, "2026-11-05");
  assert.deepEqual(request.lines, [{ product_id: cafe.id, description: "Café 500g", quantity: "2", unit_price: "10.1", discount: "0.00" }]);
  assert.equal(request.notes, "Pedido WooCommerce #123 (centro)");
  // The same order is the same request, so sending it again is the same sale.
  assert.deepEqual(saleOf(structuredClone(found), options), { request });
  // What changes in the store afterwards without changing what was bought does not change the request.
  assert.deepEqual(saleOf({ ...found, status: "completed", date_modified_gmt: "2026-12-01T00:00:00" }, options), { request });
});

test("two stores of one company keep their orders apart", () => {
  assert.notEqual(referenceOf("centro", 123), referenceOf("norte", 123));
  assert.ok(referenceOf("a".repeat(40), 9_999_999_999).length <= 100);
  assert.equal(orderIdOf("centro", "woocommerce:centro:123"), 123);
  for (const foreign of ["woocommerce:norte:123", "pdv-9", "woocommerce:centro:12x", "woocommerce:centro:", null, undefined]) assert.equal(orderIdOf("centro", foreign), null, String(foreign));
});

test("an order that cannot be a sale says why and makes nothing", () => {
  const cases = [
    [order({ currency: "USD" }), /USD/],
    [order({ line_items: [] }), /no items/],
    [order({ line_items: [{ name: "Sem código", sku: "", quantity: 1, subtotal: "1.00", total: "1.00" }] }), /no SKU/],
    [order({ line_items: [{ name: "Outro", sku: "xx-1", quantity: 1, subtotal: "1.00", total: "1.00" }] }), /no product with the SKU XX-1/],
    [order({ line_items: [{ name: "Café", sku: "cafe-500", quantity: 1, subtotal: "1.00", total: "2.00" }] }), /above its subtotal/],
    [order({ date_created: "" }), /no date/],
  ];
  for (const [bad, why] of cases) assert.match(saleOf(bad, options).problem, why);
});

test("a SKU is matched without regard to case, as the ERP keeps it in upper case", () => {
  const lower = order({ line_items: [{ name: "Café", sku: " cafe-500 ", quantity: 1, subtotal: "10.10", total: "10.10" }] });
  assert.equal(saleOf(lower, options).request.lines[0].product_id, cafe.id);
});
