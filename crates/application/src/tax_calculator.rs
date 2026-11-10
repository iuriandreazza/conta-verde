use conta_verde_domain::{
    Amount, DocumentTax, DomainError, ItemTax, TaxRule, document_tax, item_tax,
};

/// Port to whatever works out the taxes of an invoice's items ([ADR 0084]). It is the place a **certified** engine, or a
/// calculation an accountant has signed for the regular regimes, would be plugged in; until one is, [`RuleRateCalculator`] works
/// from the company's own rates and **no issue uses it** (the regimes other than the Simples Nacional stay refused).
///
/// An implementation is pure: the same rule and values always give the same taxes, in exact decimals, with no I/O and no clock.
pub trait TaxCalculator: Send + Sync {
    /// The taxes of one item whose value is `gross` and whose discount is `discount`, both in cents.
    fn item(&self, rule: &TaxRule, gross: Amount, discount: Amount)
    -> Result<ItemTax, DomainError>;

    /// What the items' taxes come to together.
    fn document(&self, items: &[ItemTax]) -> Result<DocumentTax, DomainError>;
}

/// The calculation that embeds no tax law: each tax is the item's value less its discount times the rate the **company's rule**
/// gives, rounded to the cent (halves away from zero), and a tax the rule gives no rate for is not calculated. What the base
/// and the rates should be is the company's and its accountant's to say (`docs/fiscal/accountant-checklist.md`).
#[derive(Debug, Default, Clone, Copy)]
pub struct RuleRateCalculator;

impl TaxCalculator for RuleRateCalculator {
    fn item(
        &self,
        rule: &TaxRule,
        gross: Amount,
        discount: Amount,
    ) -> Result<ItemTax, DomainError> {
        item_tax(rule, gross, discount)
    }

    fn document(&self, items: &[ItemTax]) -> Result<DocumentTax, DomainError> {
        document_tax(items)
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_domain::{
        CompanyId, FiscalSettings, FiscalSettingsDraft, NatureKind, OperationNatureDraft,
        TaxRegime, TaxRuleDraft,
    };

    use super::*;

    fn rule_with(icms: &str, pis: Option<&str>) -> TaxRule {
        let draft = FiscalSettingsDraft {
            legal_name: "Verde Comércio Ltda".into(),
            cnpj: "11.222.333/0001-81".into(),
            state_registration: None,
            municipal_registration: None,
            address: conta_verde_domain::AddressDraft {
                postal_code: "01310100".into(),
                street: "Avenida Paulista".into(),
                number: Some("1000".into()),
                complement: None,
                district: None,
                city: "São Paulo".into(),
                state: "SP".into(),
            },
            regime: TaxRegime::SimplesNacional,
            environment: conta_verde_domain::FiscalEnvironment::Homologation,
            nfe_series: 1,
            nfce_series: 1,
            nfse_series: 1,
            certificate_valid_until: None,
            city_ibge_code: None,
            natures: vec![OperationNatureDraft {
                code: "sale".into(),
                name: "Venda".into(),
                kind: NatureKind::Product,
                cfop_in_state: Some("5102".into()),
                cfop_interstate: Some("6102".into()),
                active: true,
                rules: vec![TaxRuleDraft {
                    regime: TaxRegime::SimplesNacional,
                    ncm_prefix: None,
                    icms_code: "102".into(),
                    pis_code: "49".into(),
                    cofins_code: "49".into(),
                    icms_rate: Some(icms.into()),
                    pis_rate: pis.map(str::to_owned),
                    cofins_rate: None,
                }],
                service: None,
            }],
        };
        FiscalSettings::new(CompanyId::generate(), draft)
            .unwrap()
            .natures()[0]
            .rule_for(TaxRegime::SimplesNacional, "49111090")
            .unwrap()
            .clone()
    }

    /// What any calculator must hold to, whoever wrote it: pure, exact, a document that adds up its items, and nothing
    /// invented for a rate that is not there.
    fn a_calculator_keeps_its_promises<C: TaxCalculator>(calculator: &C) {
        let rule = rule_with("12", Some("0.65"));
        let value = |text: &str| Amount::in_cents(text).unwrap();
        let first = calculator
            .item(&rule, value("19.99"), value("0.99"))
            .unwrap();
        // The same input, the same taxes.
        assert_eq!(
            first,
            calculator
                .item(&rule, value("19.99"), value("0.99"))
                .unwrap()
        );
        let second = calculator.item(&rule, value("5.05"), value("0")).unwrap();
        let total = calculator.document(&[first, second]).unwrap();
        assert_eq!(
            total.icms.as_decimal(),
            first.icms.unwrap().amount.as_decimal() + second.icms.unwrap().amount.as_decimal()
        );
        assert_eq!(
            total.base.as_decimal(),
            first.base.as_decimal() + second.base.as_decimal()
        );
        assert_eq!(first.cofins, None, "no rate, no amount");
        assert_eq!(calculator.document(&[]).unwrap().icms, Amount::ZERO);
        assert!(calculator.item(&rule, value("1"), value("2")).is_err());
    }

    #[test]
    fn the_rule_rate_calculator_keeps_the_promises_of_a_calculator() {
        a_calculator_keeps_its_promises(&RuleRateCalculator);
    }
}
