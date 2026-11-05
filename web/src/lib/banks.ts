/**
 * The banks most small businesses in Brazil use, by the code the Central Bank gives them (COMPE), so
 * an account shows who holds it and not only a number. A code that is not here still shows as typed.
 */
const BANKS: Record<string, string> = {
  "001": "Banco do Brasil",
  "033": "Santander",
  "041": "Banrisul",
  "070": "BRB",
  "077": "Inter",
  "104": "Caixa Econômica",
  "208": "BTG Pactual",
  "212": "Original",
  "237": "Bradesco",
  "260": "Nubank",
  "336": "C6 Bank",
  "341": "Itaú",
  "422": "Safra",
  "748": "Sicredi",
  "756": "Sicoob",
};

export function bankName(code: string): string | undefined {
  return BANKS[code];
}
