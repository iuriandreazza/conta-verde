use rust_decimal::Decimal;

use crate::line::MAX_LINES;
use crate::name::{non_blank, required_text};
use crate::{Amount, CalendarDate, Document, DomainError, LineDraft, ProductId, round_to_cents};

const KEY_LEN: usize = 44;

/// The 44 characters that name an NF-e: where it was issued, when, by whom, its number, and a check digit
/// ([ADR 0057]). Only a key whose check digit is right is built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessKey(String);

impl AccessKey {
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        let text: String = raw
            .chars()
            .filter(|c| !c.is_whitespace())
            .map(|c| c.to_ascii_uppercase())
            .collect();
        if text.len() != KEY_LEN {
            return Err(DomainError::InvalidAccessKey);
        }
        // The issuer's registry number, in positions 6 to 20, may hold letters (an alphanumeric CNPJ); the rest
        // is digits. Every character counts as its ASCII code less 48, like the CNPJ's own check digits.
        let values: Vec<u32> = text
            .chars()
            .enumerate()
            .map(|(index, c)| {
                let allowed =
                    c.is_ascii_digit() || ((6..20).contains(&index) && c.is_ascii_uppercase());
                allowed
                    .then(|| u32::from(c) - u32::from('0'))
                    .ok_or(DomainError::InvalidAccessKey)
            })
            .collect::<Result<_, _>>()?;
        if check_digit(&values[..KEY_LEN - 1]) != values[KEY_LEN - 1] {
            return Err(DomainError::InvalidAccessKey);
        }
        Ok(Self(text))
    }

    /// The key of a document an issuer makes, from what it is made of: the state's IBGE code, the month, the issuer's
    /// CNPJ (14 characters), the model (55 for an NF-e), series, number, the emission type (1 is normal) and an
    /// eight-digit code the issuer picks. The check digit is worked out, so only a right key is built.
    #[allow(clippy::too_many_arguments)]
    pub fn from_parts(
        state_code: u8,
        issued_on: CalendarDate,
        issuer: &str,
        model: u8,
        series: u16,
        number: u32,
        emission_type: u8,
        numeric_code: u32,
    ) -> Result<Self, DomainError> {
        // Every part has its own width: a wider one would shift the others and still make a 43-character body.
        let fits = state_code < 100
            && model < 100
            && series < 1_000
            && number < 1_000_000_000
            && emission_type < 10
            && numeric_code < 100_000_000
            && issuer.len() == 14
            && issuer
                .bytes()
                .all(|byte| byte.is_ascii_digit() || byte.is_ascii_uppercase());
        if !fits {
            return Err(DomainError::InvalidAccessKey);
        }
        let body = format!(
            "{state_code:02}{:02}{:02}{issuer}{model:02}{series:03}{number:09}{emission_type}{numeric_code:08}",
            issued_on.year() % 100,
            issued_on.month()
        );
        let values: Vec<u32> = body
            .bytes()
            .map(|byte| u32::from(byte) - u32::from(b'0'))
            .collect();
        Self::parse(&format!("{body}{}", check_digit(&values)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The issuer's registry number as the key holds it: 14 characters (a CPF is padded with zeros on the left).
    pub fn issuer(&self) -> &str {
        &self.0[6..20]
    }

    /// The model of the document, two digits: 55 for an NF-e, 65 for an NFC-e.
    pub fn model(&self) -> &str {
        &self.0[20..22]
    }

    /// The series, three digits.
    pub fn series(&self) -> &str {
        &self.0[22..25]
    }

    /// The number, nine digits.
    pub fn number(&self) -> &str {
        &self.0[25..34]
    }
}

/// The check digit of the first 43 characters of a key: modulo 11, with weights 2 to 9 from the right.
fn check_digit(digits: &[u32]) -> u32 {
    let sum: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(index, digit)| digit * (2 + (index as u32 % 8)))
        .sum();
    match sum % 11 {
        0 | 1 => 0,
        rest => 11 - rest,
    }
}

/// One item of an invoice as the XML says it, before it is checked.
#[derive(Debug, Clone, Default)]
pub struct InvoiceItemDraft {
    pub number: String,
    /// The supplier's code for the product.
    pub code: String,
    pub ean: Option<String>,
    pub description: String,
    pub ncm: Option<String>,
    pub unit: String,
    pub quantity: String,
    pub unit_value: String,
    pub product_value: String,
    pub discount: String,
    pub freight: String,
    pub insurance: String,
    pub other: String,
    pub ipi: String,
    pub icms_st: String,
    pub fcp_st: String,
    pub import_tax: String,
    /// What the ICMS of the item is, for the person to see: it is not part of the cost.
    pub icms: String,
}

/// The totals of an invoice as the XML says them.
#[derive(Debug, Clone, Default)]
pub struct InvoiceTotalsDraft {
    pub products: String,
    pub discount: String,
    pub freight: String,
    pub insurance: String,
    pub other: String,
    pub ipi: String,
    pub icms_st: String,
    pub fcp_st: String,
    pub import_tax: String,
    pub icms_desoneration: String,
    pub icms: String,
    pub invoice: String,
}

#[derive(Debug, Clone)]
pub struct InvoiceInstallmentDraft {
    pub due_date: String,
    pub amount: String,
}

/// An NF-e as it was read from its XML: plain text, nothing checked ([ADR 0057]).
#[derive(Debug, Clone)]
pub struct InvoiceDraft {
    pub access_key: String,
    pub number: String,
    pub series: String,
    /// The day it was issued, `YYYY-MM-DD`.
    pub issued_on: String,
    pub issuer_document: String,
    pub issuer_name: String,
    pub items: Vec<InvoiceItemDraft>,
    pub totals: InvoiceTotalsDraft,
    pub installments: Vec<InvoiceInstallmentDraft>,
}

/// An item of a checked invoice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvoiceItem {
    number: u16,
    code: String,
    ean: Option<String>,
    description: String,
    ncm: Option<String>,
    unit: String,
    quantity: Amount,
    unit_value: String,
    product_value: Decimal,
    discount: Decimal,
    /// Freight, insurance, other expenses, IPI, ICMS ST, FCP ST and import tax of the item.
    charges: Decimal,
    ipi: Decimal,
    icms_st: Decimal,
    icms: Decimal,
}

impl InvoiceItem {
    pub fn number(&self) -> u16 {
        self.number
    }

    pub fn code(&self) -> &str {
        &self.code
    }

    pub fn ean(&self) -> Option<&str> {
        self.ean.as_deref()
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn ncm(&self) -> Option<&str> {
        self.ncm.as_deref()
    }

    pub fn unit(&self) -> &str {
        &self.unit
    }

    pub fn quantity(&self) -> Amount {
        self.quantity
    }

    pub fn unit_value(&self) -> &str {
        &self.unit_value
    }

    pub fn product_value(&self) -> Decimal {
        self.product_value
    }

    pub fn discount(&self) -> Decimal {
        self.discount
    }

    pub fn ipi(&self) -> Decimal {
        self.ipi
    }

    pub fn icms_st(&self) -> Decimal {
        self.icms_st
    }

    pub fn icms(&self) -> Decimal {
        self.icms
    }
}

/// A supplier's NF-e that has been checked: its access key is a key, the issuer is who the key says, the items add
/// up to the totals and the totals to the invoice's value, and what it is to be paid in adds up to that value
/// ([ADR 0057]). Nothing here asks a tax authority anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invoice {
    access_key: AccessKey,
    number: String,
    series: String,
    issued_on: CalendarDate,
    issuer: Document,
    issuer_name: String,
    items: Vec<InvoiceItem>,
    total: Decimal,
    installments: Vec<(CalendarDate, Decimal)>,
}

fn cents(raw: &str, field: &'static str) -> Result<Decimal, DomainError> {
    let text = raw.trim();
    let text = if text.is_empty() { "0" } else { text };
    Amount::in_cents(text)
        .map(|amount| amount.as_decimal())
        .ok_or(DomainError::InvalidInvoiceAmount(field))
}

impl Invoice {
    pub fn new(draft: InvoiceDraft) -> Result<Self, DomainError> {
        let access_key = AccessKey::parse(&draft.access_key)?;
        let issuer = Document::parse(&draft.issuer_document)?;
        // The key names its issuer, number and series: an invoice whose key says another is not what it claims.
        let key_issuer = match &issuer {
            Document::Cnpj(text) => text.clone(),
            Document::Cpf(text) => format!("000{text}"),
        };
        if access_key.issuer() != key_issuer {
            return Err(DomainError::InvoiceKeyMismatch("issuer"));
        }
        let number = required_text(&draft.number, 9, DomainError::InvoiceKeyMismatch("number"))?;
        let series = required_text(&draft.series, 3, DomainError::InvoiceKeyMismatch("series"))?;
        if number.parse::<u64>().ok() != access_key.number().parse::<u64>().ok() {
            return Err(DomainError::InvoiceKeyMismatch("number"));
        }
        if series.parse::<u64>().ok() != access_key.series().parse::<u64>().ok() {
            return Err(DomainError::InvoiceKeyMismatch("series"));
        }
        let issued_on =
            CalendarDate::parse(&draft.issued_on).ok_or(DomainError::InvalidDate("issued_on"))?;
        let issuer_name = required_text(
            &draft.issuer_name,
            200,
            DomainError::InvalidInvoiceAmount("issuer name"),
        )?;
        if draft.items.is_empty() {
            return Err(DomainError::InvoiceHasNoItems);
        }
        if draft.items.len() > MAX_LINES {
            return Err(DomainError::TooManyInvoiceItems);
        }
        let mut items = Vec::with_capacity(draft.items.len());
        for (index, item) in draft.items.iter().enumerate() {
            items.push(Self::item(index, item)?);
        }

        let totals = &draft.totals;
        let sum = |field: fn(&InvoiceItemDraft) -> &str,
                   name: &'static str|
         -> Result<Decimal, DomainError> {
            draft
                .items
                .iter()
                .map(|item| cents(field(item), name))
                .sum()
        };
        let declared = |raw: &str, name: &'static str| cents(raw, name);
        let products = declared(&totals.products, "total of products")?;
        if sum(|item| &item.product_value, "value of an item")? != products {
            return Err(DomainError::InvoiceTotalsMismatch("products"));
        }
        let discount = declared(&totals.discount, "total discount")?;
        if sum(|item| &item.discount, "discount of an item")? != discount {
            return Err(DomainError::InvoiceTotalsMismatch("discount"));
        }
        let invoice_value = declared(&totals.invoice, "total of the invoice")?;
        // The invoice is worth the products less discounts and exemptions, plus what is charged on top.
        let expected = products - discount - declared(&totals.icms_desoneration, "ICMS exemption")?
            + declared(&totals.icms_st, "ICMS ST")?
            + declared(&totals.fcp_st, "FCP ST")?
            + declared(&totals.freight, "freight")?
            + declared(&totals.insurance, "insurance")?
            + declared(&totals.other, "other expenses")?
            + declared(&totals.import_tax, "import tax")?
            + declared(&totals.ipi, "IPI")?;
        if invoice_value != expected || invoice_value <= Decimal::ZERO {
            return Err(DomainError::InvoiceTotalsMismatch("invoice"));
        }

        let mut installments: Vec<(CalendarDate, Decimal)> = Vec::new();
        for installment in &draft.installments {
            let due = CalendarDate::parse(&installment.due_date)
                .ok_or(DomainError::InvalidDate("due_date"))?;
            let amount = cents(&installment.amount, "installment")?;
            if amount <= Decimal::ZERO || installments.last().is_some_and(|(last, _)| due < *last) {
                return Err(DomainError::InvoiceInstallmentsMismatch);
            }
            installments.push((due, amount));
        }
        if !installments.is_empty()
            && installments
                .iter()
                .map(|(_, amount)| *amount)
                .sum::<Decimal>()
                != invoice_value
        {
            return Err(DomainError::InvoiceInstallmentsMismatch);
        }

        Ok(Self {
            access_key,
            number,
            series,
            issued_on,
            issuer,
            issuer_name,
            items,
            total: invoice_value,
            installments,
        })
    }

    fn item(index: usize, draft: &InvoiceItemDraft) -> Result<InvoiceItem, DomainError> {
        let number = u16::try_from(index + 1).map_err(|_| DomainError::TooManyInvoiceItems)?;
        let quantity = Amount::parse(draft.quantity.trim())
            .filter(|quantity| quantity.as_decimal() > Decimal::ZERO)
            .ok_or(DomainError::InvalidInvoiceAmount("quantity"))?;
        if quantity.as_decimal().scale() > 3 {
            return Err(DomainError::InvoiceQuantityTooPrecise);
        }
        let charge = |raw: &str, name: &'static str| cents(raw, name);
        let ipi = charge(&draft.ipi, "IPI of an item")?;
        let icms_st = charge(&draft.icms_st, "ICMS ST of an item")?;
        let charges = charge(&draft.freight, "freight of an item")?
            + charge(&draft.insurance, "insurance of an item")?
            + charge(&draft.other, "other expenses of an item")?
            + charge(&draft.fcp_st, "FCP ST of an item")?
            + charge(&draft.import_tax, "import tax of an item")?
            + ipi
            + icms_st;
        let product_value = charge(&draft.product_value, "value of an item")?;
        let discount = charge(&draft.discount, "discount of an item")?;
        if discount > product_value {
            return Err(DomainError::InvalidInvoiceAmount("discount of an item"));
        }
        Ok(InvoiceItem {
            number,
            code: required_text(&draft.code, 60, DomainError::InvalidInvoiceAmount("code"))?,
            ean: non_blank(draft.ean.as_deref().unwrap_or_default())
                .filter(|ean| !ean.eq_ignore_ascii_case("SEM GTIN"))
                .map(str::to_owned),
            description: required_text(
                &draft.description,
                200,
                DomainError::InvalidInvoiceAmount("description"),
            )?,
            ncm: non_blank(draft.ncm.as_deref().unwrap_or_default()).map(str::to_owned),
            unit: required_text(&draft.unit, 6, DomainError::InvalidInvoiceAmount("unit"))?,
            quantity,
            unit_value: draft.unit_value.trim().to_owned(),
            product_value,
            discount,
            charges,
            ipi,
            icms_st,
            icms: charge(&draft.icms, "ICMS of an item")?,
        })
    }

    /// What each item costs: its products less its discount plus the charges on it (freight, insurance, IPI, ICMS ST
    /// and the like), **and whatever is left to reach the invoice's value** shared in proportion, the last item taking
    /// the rest, so the costs add up to what the supplier is paid exactly ([ADR 0057]).
    pub fn costs(&self) -> Vec<Decimal> {
        let mut costs: Vec<Decimal> = self
            .items
            .iter()
            .map(|item| item.product_value - item.discount + item.charges)
            .collect();
        let residual = self.total - costs.iter().sum::<Decimal>();
        if residual.is_zero() {
            return costs.into_iter().map(|cost| cost.normalize()).collect();
        }
        let weight: Decimal = costs.iter().sum();
        let last = costs.len() - 1;
        let mut shared = Decimal::ZERO;
        for (index, cost) in costs.iter_mut().enumerate() {
            let part = if index == last {
                residual - shared
            } else if weight.is_zero() {
                Decimal::ZERO
            } else {
                // The cost is cut to the cent; the last item takes what the cuts leave.
                (residual * *cost / weight).trunc_with_scale(2)
            };
            shared += part;
            *cost += part;
        }
        costs.into_iter().map(|cost| cost.normalize()).collect()
    }

    /// The lines of the purchase an invoice becomes, given the product each item is. The unit cost is the cost
    /// of the item over its quantity, rounded **up** to six decimals, and the cents that makes the line worth too
    /// much are taken as a discount, so each line is worth exactly what the item costs.
    pub fn lines(&self, products: &[ProductId]) -> Vec<LineDraft> {
        self.items
            .iter()
            .zip(self.costs())
            .zip(products)
            .map(|((item, cost), product)| {
                let quantity = item.quantity.as_decimal();
                let mut unit = (cost / quantity)
                    .round_dp_with_strategy(6, rust_decimal::RoundingStrategy::ToPositiveInfinity);
                // Floors at a cent over six decimals never fall below the cost; guard the exact case anyway.
                if round_to_cents(quantity * unit) < cost {
                    unit += Decimal::new(1, 6);
                }
                let gross = round_to_cents(quantity * unit);
                LineDraft {
                    product_id: *product,
                    description: item.description.chars().take(200).collect(),
                    quantity: quantity.normalize().to_string(),
                    unit_price: unit.normalize().to_string(),
                    discount: (gross - cost).normalize().to_string(),
                }
            })
            .collect()
    }

    pub fn access_key(&self) -> &AccessKey {
        &self.access_key
    }

    pub fn number(&self) -> &str {
        &self.number
    }

    pub fn series(&self) -> &str {
        &self.series
    }

    pub fn issued_on(&self) -> CalendarDate {
        self.issued_on
    }

    pub fn issuer(&self) -> &Document {
        &self.issuer
    }

    pub fn issuer_name(&self) -> &str {
        &self.issuer_name
    }

    pub fn items(&self) -> &[InvoiceItem] {
        &self.items
    }

    /// What the supplier is paid in all, in cents.
    pub fn total(&self) -> Decimal {
        self.total
    }

    /// When and how much the invoice says is paid; empty when it does not say.
    pub fn installments(&self) -> &[(CalendarDate, Decimal)] {
        &self.installments
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A key whose check digit is right, for the issuer, series and number given.
    pub(crate) fn key_for(cnpj: &str, series: u32, number: u32) -> String {
        let body = format!("352611{cnpj:0>14}55{series:03}{number:09}1{:08}", 12345678);
        let values: Vec<u32> = body
            .chars()
            .map(|c| u32::from(c) - u32::from('0'))
            .collect();
        format!("{body}{}", check_digit(&values))
    }

    const CNPJ: &str = "11222333000181";

    #[test]
    fn a_key_is_made_from_its_parts_with_the_right_check_digit() {
        let made = AccessKey::from_parts(
            35,
            CalendarDate::parse("2026-11-09").unwrap(),
            CNPJ,
            55,
            1,
            4321,
            1,
            12_345_678,
        )
        .unwrap();
        assert_eq!(made.as_str(), key_for(CNPJ, 1, 4321));
        assert_eq!(made.number(), "000004321");
        assert_eq!(made.series(), "001");
        // An alphanumeric CNPJ counts like its own check digits do, and one of the wrong length is refused.
        let alphanumeric = AccessKey::from_parts(
            35,
            CalendarDate::parse("2026-11-09").unwrap(),
            "12ABC34501DE35",
            55,
            1,
            1,
            1,
            1,
        )
        .unwrap();
        assert_eq!(alphanumeric.issuer(), "12ABC34501DE35");
        // A part wider than its place would shift the others: refused, not misaligned.
        for (series, number, code) in [(1000, 1, 1), (1, 1_000_000_000, 1), (1, 1, 100_000_000)] {
            assert!(
                AccessKey::from_parts(
                    35,
                    CalendarDate::parse("2026-11-09").unwrap(),
                    CNPJ,
                    55,
                    series,
                    number,
                    1,
                    code
                )
                .is_err()
            );
        }
        assert!(
            AccessKey::from_parts(
                35,
                CalendarDate::parse("2026-11-09").unwrap(),
                "123",
                55,
                1,
                1,
                1,
                1
            )
            .is_err()
        );
    }

    fn item(code: &str, quantity: &str, value: &str) -> InvoiceItemDraft {
        InvoiceItemDraft {
            number: "1".into(),
            code: code.into(),
            description: format!("Produto {code}"),
            unit: "UN".into(),
            quantity: quantity.into(),
            unit_value: "0".into(),
            product_value: value.into(),
            ..InvoiceItemDraft::default()
        }
    }

    pub(crate) fn draft() -> InvoiceDraft {
        InvoiceDraft {
            access_key: key_for(CNPJ, 1, 4321),
            number: "4321".into(),
            series: "1".into(),
            issued_on: "2026-11-06".into(),
            issuer_document: CNPJ.into(),
            issuer_name: "Torrefação Boa Ltda".into(),
            items: vec![item("A", "10", "100.00"), item("B", "3", "9.99")],
            totals: InvoiceTotalsDraft {
                products: "109.99".into(),
                invoice: "109.99".into(),
                ..InvoiceTotalsDraft::default()
            },
            installments: Vec::new(),
        }
    }

    #[test]
    fn the_check_digit_of_a_key_is_modulo_11_with_weights_2_to_9() {
        // The example of the layout manual: 4313... is not needed, a computed one must round trip and a changed
        // digit must not.
        let key = key_for(CNPJ, 1, 4321);
        assert!(AccessKey::parse(&key).is_ok());
        let mut other = key.clone();
        other.replace_range(10..11, if &key[10..11] == "1" { "2" } else { "1" });
        assert_eq!(
            AccessKey::parse(&other).unwrap_err(),
            DomainError::InvalidAccessKey
        );
        for bad in [
            "",
            "123",
            &key[..43],
            &format!("{key}0"),
            &key.replace('3', "x"),
        ] {
            assert_eq!(
                AccessKey::parse(bad).unwrap_err(),
                DomainError::InvalidAccessKey
            );
        }
        // Spaces between the groups are what a person copies.
        let grouped: Vec<String> = key
            .as_bytes()
            .chunks(4)
            .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
            .collect();
        assert_eq!(AccessKey::parse(&grouped.join(" ")).unwrap().as_str(), key);
    }

    #[test]
    fn an_alphanumeric_cnpj_may_issue_an_invoice() {
        // 12.ABC.345/01DE-35 is a CNPJ with letters, issued since July 2026.
        let mut lettered = draft();
        lettered.issuer_document = "12ABC34501DE35".into();
        lettered.access_key = key_for("12ABC34501DE35", 1, 4321);
        assert_eq!(
            Invoice::new(lettered.clone()).unwrap().issuer().as_str(),
            "12ABC34501DE35"
        );
        // Letters are only where the issuer's number is: anywhere else, or a wrong digit, is not a key.
        let mut elsewhere = lettered.access_key.clone();
        elsewhere.replace_range(30..31, "A");
        assert_eq!(
            AccessKey::parse(&elsewhere).unwrap_err(),
            DomainError::InvalidAccessKey
        );
        lettered.access_key.replace_range(7..8, "Z");
        assert_eq!(
            Invoice::new(lettered).unwrap_err(),
            DomainError::InvalidAccessKey
        );
    }

    #[test]
    fn a_checked_invoice_has_the_issuer_the_key_says_and_totals_that_add_up() {
        let invoice = Invoice::new(draft()).unwrap();
        assert_eq!(invoice.total().to_string(), "109.99");
        assert_eq!(invoice.items().len(), 2);
        assert_eq!(invoice.issuer().as_str(), CNPJ);

        let mut other_issuer = draft();
        other_issuer.issuer_document = "11444777000161".into();
        assert_eq!(
            Invoice::new(other_issuer).unwrap_err(),
            DomainError::InvoiceKeyMismatch("issuer")
        );
        let mut other_number = draft();
        other_number.number = "4322".into();
        assert_eq!(
            Invoice::new(other_number).unwrap_err(),
            DomainError::InvoiceKeyMismatch("number")
        );
        let mut wrong_total = draft();
        wrong_total.totals.products = "110.00".into();
        assert_eq!(
            Invoice::new(wrong_total).unwrap_err(),
            DomainError::InvoiceTotalsMismatch("products")
        );
        let mut wrong_value = draft();
        wrong_value.totals.invoice = "109.98".into();
        assert_eq!(
            Invoice::new(wrong_value).unwrap_err(),
            DomainError::InvoiceTotalsMismatch("invoice")
        );
        let mut no_items = draft();
        no_items.items.clear();
        assert_eq!(
            Invoice::new(no_items).unwrap_err(),
            DomainError::InvoiceHasNoItems
        );
        let mut precise = draft();
        precise.items[0].quantity = "1.2345".into();
        assert_eq!(
            Invoice::new(precise).unwrap_err(),
            DomainError::InvoiceQuantityTooPrecise
        );
    }

    #[test]
    fn the_invoice_is_worth_the_products_less_discounts_plus_what_is_charged_on_top() {
        let mut charged = draft();
        charged.items[0].freight = "5.00".into();
        charged.items[0].ipi = "2.50".into();
        charged.items[1].discount = "0.99".into();
        charged.totals.freight = "5.00".into();
        charged.totals.ipi = "2.50".into();
        charged.totals.discount = "0.99".into();
        charged.totals.invoice = "116.50".into();
        let invoice = Invoice::new(charged).unwrap();
        // 100 + 5 + 2.5 = 107.5 and 9.99 - 0.99 = 9: 116.5 in all.
        let costs: Vec<String> = invoice.costs().iter().map(|c| c.to_string()).collect();
        assert_eq!(costs, ["107.5", "9"]);
    }

    #[test]
    fn what_the_items_do_not_carry_is_shared_so_the_costs_add_up_to_the_invoice() {
        // Freight of 3.00 on the invoice that no item carries: 3 shared over 100 and 9.99 in proportion.
        let mut shared = draft();
        shared.totals.freight = "3.00".into();
        shared.totals.invoice = "112.99".into();
        let invoice = Invoice::new(shared).unwrap();
        let costs = invoice.costs();
        assert_eq!(costs.iter().sum::<Decimal>().to_string(), "112.99");
        // 3 x 100 / 109.99 = 2.727 -> 2.72 for the first, the rest (0.28) for the last.
        assert_eq!(costs[0].to_string(), "102.72");
        assert_eq!(costs[1].to_string(), "10.27");
    }

    #[test]
    fn the_installments_the_invoice_gives_add_up_to_its_value_and_never_go_back_in_time() {
        let mut paid = draft();
        paid.installments = vec![
            InvoiceInstallmentDraft {
                due_date: "2026-12-06".into(),
                amount: "55.00".into(),
            },
            InvoiceInstallmentDraft {
                due_date: "2027-01-06".into(),
                amount: "54.99".into(),
            },
        ];
        let invoice = Invoice::new(paid.clone()).unwrap();
        assert_eq!(invoice.installments().len(), 2);
        paid.installments[1].amount = "54.98".into();
        assert_eq!(
            Invoice::new(paid.clone()).unwrap_err(),
            DomainError::InvoiceInstallmentsMismatch
        );
        paid.installments[1].amount = "54.99".into();
        paid.installments[1].due_date = "2026-11-06".into();
        assert_eq!(
            Invoice::new(paid).unwrap_err(),
            DomainError::InvoiceInstallmentsMismatch
        );
    }

    #[test]
    fn a_line_is_worth_exactly_what_its_item_costs() {
        let invoice = Invoice::new(draft()).unwrap();
        let products = [ProductId::generate(), ProductId::generate()];
        let lines = invoice.lines(&products);
        // 100 / 10 = 10; 9.99 / 3 = 3.33.
        assert_eq!(lines[0].unit_price, "10");
        assert_eq!(lines[0].discount, "0");
        assert_eq!(lines[1].unit_price, "3.33");
        let mut awkward = draft();
        awkward.items = vec![item("A", "3", "10.00")];
        awkward.totals.products = "10.00".into();
        awkward.totals.invoice = "10.00".into();
        let invoice = Invoice::new(awkward).unwrap();
        let line = crate::Line::new(&invoice.lines(&products[..1])[0]).unwrap();
        // 10 / 3 rounds up to 3.333334; 3 x that is 10.000002 -> 10.00, so nothing to discount.
        assert_eq!(line.total().to_string(), "10");
        let mut odd = draft();
        odd.items = vec![item("A", "7", "0.10")];
        odd.totals.products = "0.10".into();
        odd.totals.invoice = "0.10".into();
        let invoice = Invoice::new(odd).unwrap();
        let line = crate::Line::new(&invoice.lines(&products[..1])[0]).unwrap();
        assert_eq!(line.total().to_string(), "0.1");
    }
}
