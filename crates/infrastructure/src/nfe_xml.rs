//! Reads a supplier's NF-e from its XML ([ADR 0057]): **parsing only**. No network, no tax authority: the document is
//! taken as it is and handed to the domain, which checks it. The parser never loads an external entity and refuses a
//! document type declaration, so a hostile file cannot make it read a file or a URL, or grow without bound, and the
//! size is limited before it sees a byte.

use conta_verde_domain::{
    InvoiceDraft, InvoiceInstallmentDraft, InvoiceItemDraft, InvoiceTotalsDraft,
};
use roxmltree::{Document, Node, ParsingOptions};

/// The most an NF-e file may be: real ones are a few tens of kilobytes.
pub const MAX_NFE_BYTES: usize = 1024 * 1024;

/// Why a file was not read as an NF-e. The messages are for the person who sent the file: they name what is wrong with
/// it and never what the parser said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NfeError {
    TooLarge,
    NotText,
    NotXml,
    /// It is XML, but not an NF-e this system takes: another document, another model, or not a normal invoice.
    Unsupported(&'static str),
    /// A part the invoice cannot do without is missing.
    Missing(&'static str),
}

impl std::fmt::Display for NfeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge => f.write_str("the file is larger than 1 MB: it is not an NF-e"),
            Self::NotText => f.write_str("the file is not text in UTF-8"),
            Self::NotXml => f.write_str("the file is not a valid XML"),
            Self::Unsupported(what) => {
                write!(f, "this file is not an NF-e that can be entered: {what}")
            }
            Self::Missing(what) => write!(f, "the NF-e has no {what}"),
        }
    }
}

impl std::error::Error for NfeError {}

fn child<'a, 'input>(node: Node<'a, 'input>, name: &str) -> Option<Node<'a, 'input>> {
    node.children()
        .find(|candidate| candidate.is_element() && candidate.tag_name().name() == name)
}

fn text(node: Node<'_, '_>, name: &str) -> Option<String> {
    child(node, name)
        .and_then(|found| found.text())
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn text_or_zero(node: Option<Node<'_, '_>>, name: &str) -> String {
    node.and_then(|node| text(node, name))
        .unwrap_or_else(|| "0".to_owned())
}

/// The first element named `name` anywhere under `node`.
fn descendant<'a, 'input>(node: Node<'a, 'input>, name: &str) -> Option<Node<'a, 'input>> {
    node.descendants()
        .find(|candidate| candidate.is_element() && candidate.tag_name().name() == name)
}

/// What an item is charged in taxes that add to its cost: `tax` inside the tax group `group` (`IPI`, `ICMS`, `II`).
fn tax(item: Node<'_, '_>, group: &str, tag: &str) -> String {
    let value = child(item, "imposto")
        .and_then(|imposto| child(imposto, group))
        .and_then(|group| descendant(group, tag))
        .and_then(|node| node.text())
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    value.unwrap_or_else(|| "0".to_owned())
}

/// Reads the XML of an NF-e (the `nfeProc` of an authorized one, or the bare `NFe`) into a draft that nothing has
/// checked yet.
pub fn parse_nfe(bytes: &[u8]) -> Result<InvoiceDraft, NfeError> {
    if bytes.len() > MAX_NFE_BYTES {
        return Err(NfeError::TooLarge);
    }
    let text_of_file = std::str::from_utf8(bytes).map_err(|_| NfeError::NotText)?;
    let text_of_file = text_of_file
        .strip_prefix('\u{feff}')
        .unwrap_or(text_of_file);
    let options = ParsingOptions {
        // A document type declaration is how entities are declared: none is taken.
        allow_dtd: false,
        nodes_limit: 50_000,
        ..ParsingOptions::default()
    };
    let document =
        Document::parse_with_options(text_of_file, options).map_err(|_| NfeError::NotXml)?;
    let root = document.root_element();
    if !matches!(root.tag_name().name(), "nfeProc" | "NFe") {
        return Err(NfeError::Unsupported("it is not an NF-e document"));
    }
    let inf = descendant(root, "infNFe").ok_or(NfeError::Missing("infNFe"))?;
    let id = inf.attribute("Id").ok_or(NfeError::Missing("access key"))?;
    let access_key = id.strip_prefix("NFe").unwrap_or(id).to_owned();
    let ide = child(inf, "ide").ok_or(NfeError::Missing("ide"))?;
    if text(ide, "mod").as_deref() != Some("55") {
        return Err(NfeError::Unsupported(
            "only the invoice of model 55 is taken",
        ));
    }
    // Only a normal invoice: a complementary one, an adjustment or a return changes another and is not a purchase.
    if text(ide, "finNFe")
        .as_deref()
        .is_some_and(|purpose| purpose != "1")
    {
        return Err(NfeError::Unsupported("only a normal invoice is taken"));
    }
    let issued = text(ide, "dhEmi")
        .or_else(|| text(ide, "dEmi"))
        .ok_or(NfeError::Missing("issue date"))?;
    let issued_on: String = issued.chars().take(10).collect();
    let emit = child(inf, "emit").ok_or(NfeError::Missing("issuer"))?;
    let issuer_document = text(emit, "CNPJ")
        .or_else(|| text(emit, "CPF"))
        .ok_or(NfeError::Missing("issuer's CNPJ or CPF"))?;

    let items = inf
        .children()
        .filter(|node| node.is_element() && node.tag_name().name() == "det")
        .map(|det| {
            let prod = child(det, "prod").ok_or(NfeError::Missing("product of an item"))?;
            Ok(InvoiceItemDraft {
                number: det.attribute("nItem").unwrap_or_default().to_owned(),
                code: text(prod, "cProd").ok_or(NfeError::Missing("code of an item"))?,
                ean: text(prod, "cEAN"),
                description: text(prod, "xProd")
                    .ok_or(NfeError::Missing("description of an item"))?,
                ncm: text(prod, "NCM"),
                unit: text(prod, "uCom").ok_or(NfeError::Missing("unit of an item"))?,
                quantity: text(prod, "qCom").ok_or(NfeError::Missing("quantity of an item"))?,
                unit_value: text(prod, "vUnCom").unwrap_or_else(|| "0".to_owned()),
                product_value: text(prod, "vProd").ok_or(NfeError::Missing("value of an item"))?,
                discount: text_or_zero(Some(prod), "vDesc"),
                freight: text_or_zero(Some(prod), "vFrete"),
                insurance: text_or_zero(Some(prod), "vSeg"),
                other: text_or_zero(Some(prod), "vOutro"),
                ipi: tax(det, "IPI", "vIPI"),
                icms_st: tax(det, "ICMS", "vICMSST"),
                fcp_st: tax(det, "ICMS", "vFCPST"),
                import_tax: tax(det, "II", "vII"),
                icms: tax(det, "ICMS", "vICMS"),
            })
        })
        .collect::<Result<Vec<_>, NfeError>>()?;

    let total = child(inf, "total")
        .and_then(|total| child(total, "ICMSTot"))
        .ok_or(NfeError::Missing("totals"))?;
    let totals = InvoiceTotalsDraft {
        products: text(total, "vProd").ok_or(NfeError::Missing("total of products"))?,
        discount: text_or_zero(Some(total), "vDesc"),
        freight: text_or_zero(Some(total), "vFrete"),
        insurance: text_or_zero(Some(total), "vSeg"),
        other: text_or_zero(Some(total), "vOutro"),
        ipi: text_or_zero(Some(total), "vIPI"),
        icms_st: text_or_zero(Some(total), "vST"),
        fcp_st: text_or_zero(Some(total), "vFCPST"),
        import_tax: text_or_zero(Some(total), "vII"),
        icms_desoneration: text_or_zero(Some(total), "vICMSDeson"),
        icms: text_or_zero(Some(total), "vICMS"),
        invoice: text(total, "vNF").ok_or(NfeError::Missing("total of the invoice"))?,
    };

    let installments = child(inf, "cobr")
        .map(|billing| {
            billing
                .children()
                .filter(|node| node.is_element() && node.tag_name().name() == "dup")
                .map(|dup| {
                    Ok(InvoiceInstallmentDraft {
                        due_date: text(dup, "dVenc")
                            .ok_or(NfeError::Missing("due date of an installment"))?,
                        amount: text(dup, "vDup")
                            .ok_or(NfeError::Missing("value of an installment"))?,
                    })
                })
                .collect::<Result<Vec<_>, NfeError>>()
        })
        .transpose()?
        .unwrap_or_default();

    Ok(InvoiceDraft {
        access_key,
        number: text(ide, "nNF").ok_or(NfeError::Missing("number"))?,
        series: text(ide, "serie").ok_or(NfeError::Missing("series"))?,
        issued_on,
        issuer_document,
        issuer_name: text(emit, "xNome").ok_or(NfeError::Missing("issuer's name"))?,
        items,
        totals,
        installments,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use conta_verde_domain::Invoice;

    use super::*;

    /// The key of an invoice of this issuer, series and number, with its check digit.
    pub(crate) fn key_for(cnpj: &str, series: u32, number: u32) -> String {
        let body = format!("352611{cnpj:0>14}55{series:03}{number:09}1{:08}", 12345678);
        let sum: u32 = body
            .chars()
            .rev()
            .enumerate()
            .map(|(index, c)| c.to_digit(10).unwrap() * (2 + (index as u32 % 8)))
            .sum();
        let digit = match sum % 11 {
            0 | 1 => 0,
            rest => 11 - rest,
        };
        format!("{body}{digit}")
    }

    /// An NF-e as a supplier's software writes it: with the namespace, authorized (`nfeProc`), two items, a tax on the
    /// first, and two installments.
    pub(crate) fn sample(cnpj: &str, series: u32, number: u32) -> String {
        let key = key_for(cnpj, series, number);
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<nfeProc xmlns="http://www.portalfiscal.inf.br/nfe" versao="4.00">
  <NFe>
    <infNFe Id="NFe{key}" versao="4.00">
      <ide><cUF>35</cUF><mod>55</mod><serie>{series}</serie><nNF>{number}</nNF>
        <dhEmi>2026-11-06T10:30:00-03:00</dhEmi><finNFe>1</finNFe></ide>
      <emit><CNPJ>{cnpj}</CNPJ><xNome>Torrefação Boa Ltda</xNome></emit>
      <det nItem="1"><prod><cProd>CAFE-500</cProd><cEAN>SEM GTIN</cEAN><xProd>Café 500g</xProd><NCM>09012100</NCM>
          <uCom>UN</uCom><qCom>10.0000</qCom><vUnCom>10.0000000000</vUnCom><vProd>100.00</vProd></prod>
        <imposto><ICMS><ICMS00><vICMS>12.00</vICMS></ICMS00></ICMS><IPI><IPITrib><vIPI>2.50</vIPI></IPITrib></IPI></imposto></det>
      <det nItem="2"><prod><cProd>PAO-1</cProd><xProd>Pão francês</xProd><NCM>19059090</NCM>
          <uCom>KG</uCom><qCom>3.0000</qCom><vUnCom>3.3300000000</vUnCom><vProd>9.99</vProd><vDesc>0.99</vDesc></prod>
        <imposto><ICMS><ICMS40><vICMS>0.00</vICMS></ICMS40></ICMS></imposto></det>
      <total><ICMSTot><vICMS>12.00</vICMS><vST>0.00</vST><vProd>109.99</vProd><vFrete>0.00</vFrete><vSeg>0.00</vSeg>
          <vDesc>0.99</vDesc><vIPI>2.50</vIPI><vOutro>0.00</vOutro><vNF>111.50</vNF></ICMSTot></total>
      <cobr><dup><nDup>001</nDup><dVenc>2026-12-06</dVenc><vDup>55.75</vDup></dup>
        <dup><nDup>002</nDup><dVenc>2027-01-06</dVenc><vDup>55.75</vDup></dup></cobr>
    </infNFe>
  </NFe>
  <protNFe><infProt><nProt>1</nProt></infProt></protNFe>
</nfeProc>"#
        )
    }

    const CNPJ: &str = "11222333000181";

    #[test]
    fn reads_what_an_authorized_invoice_says_and_the_domain_accepts_it() {
        let draft = parse_nfe(sample(CNPJ, 1, 4321).as_bytes()).unwrap();
        assert_eq!(draft.number, "4321");
        assert_eq!(draft.issued_on, "2026-11-06");
        assert_eq!(draft.issuer_document, CNPJ);
        assert_eq!(draft.items.len(), 2);
        assert_eq!(draft.items[0].ipi, "2.50");
        assert_eq!(draft.items[0].icms, "12.00");
        assert_eq!(draft.items[0].ean.as_deref(), Some("SEM GTIN"));
        assert_eq!(draft.installments.len(), 2);
        let invoice = Invoice::new(draft).unwrap();
        assert_eq!(invoice.total().to_string(), "111.5");
        // 100 + 2.5 of IPI, and 9.99 - 0.99.
        let costs: Vec<String> = invoice.costs().iter().map(|c| c.to_string()).collect();
        assert_eq!(costs, ["102.5", "9"]);
    }

    #[test]
    fn a_bare_nfe_without_the_authorization_is_read_too() {
        let wrapped = sample(CNPJ, 1, 4321);
        let start = wrapped.find("<NFe>").unwrap();
        let end = wrapped.find("</NFe>").unwrap() + "</NFe>".len();
        assert!(parse_nfe(&wrapped.as_bytes()[start..end]).is_ok());
    }

    #[test]
    fn hostile_files_are_refused_before_they_cost_anything() {
        // An entity that points at a file, and one that expands onto itself.
        let xxe = format!(
            "<?xml version=\"1.0\"?><!DOCTYPE nfeProc [<!ENTITY x SYSTEM \"file:///etc/passwd\">]>{}",
            sample(CNPJ, 1, 4321)
                .split_once("?>")
                .unwrap()
                .1
                .replace("Café", "&x;")
        );
        assert_eq!(parse_nfe(xxe.as_bytes()).unwrap_err(), NfeError::NotXml);
        let bomb = "<!DOCTYPE a [<!ENTITY b \"aaaaaaaaaa\"><!ENTITY c \"&b;&b;&b;&b;&b;&b;&b;&b;&b;&b;\">]><a>&c;</a>";
        assert_eq!(parse_nfe(bomb.as_bytes()).unwrap_err(), NfeError::NotXml);
        // Too big, not UTF-8, truncated, not XML at all, and another document.
        assert_eq!(
            parse_nfe(&vec![b' '; MAX_NFE_BYTES + 1]).unwrap_err(),
            NfeError::TooLarge
        );
        assert_eq!(
            parse_nfe(&[0xff, 0xfe, 0x00]).unwrap_err(),
            NfeError::NotText
        );
        let whole = sample(CNPJ, 1, 4321);
        assert_eq!(
            parse_nfe(whole.as_bytes().get(..whole.len() / 2).unwrap()).unwrap_err(),
            NfeError::NotXml
        );
        assert_eq!(parse_nfe(b"not xml").unwrap_err(), NfeError::NotXml);
        assert!(matches!(
            parse_nfe(b"<html><body/></html>").unwrap_err(),
            NfeError::Unsupported(_)
        ));
        // A deep nesting does not get past the node limit.
        let deep = format!("{}{}", "<a>".repeat(60_000), "</a>".repeat(60_000));
        assert_eq!(parse_nfe(deep.as_bytes()).unwrap_err(), NfeError::NotXml);
    }

    #[test]
    fn only_a_normal_invoice_of_model_55_is_taken() {
        let base = sample(CNPJ, 1, 4321);
        assert!(matches!(
            parse_nfe(base.replace("<mod>55</mod>", "<mod>65</mod>").as_bytes()).unwrap_err(),
            NfeError::Unsupported(_)
        ));
        assert!(matches!(
            parse_nfe(
                base.replace("<finNFe>1</finNFe>", "<finNFe>4</finNFe>")
                    .as_bytes()
            )
            .unwrap_err(),
            NfeError::Unsupported(_)
        ));
        assert_eq!(
            parse_nfe(base.replace("<vNF>111.50</vNF>", "").as_bytes()).unwrap_err(),
            NfeError::Missing("total of the invoice")
        );
        assert_eq!(
            parse_nfe(base.replace("<xProd>Café 500g</xProd>", "").as_bytes()).unwrap_err(),
            NfeError::Missing("description of an item")
        );
    }

    #[test]
    fn the_domain_refuses_what_the_xml_got_wrong() {
        let base = sample(CNPJ, 1, 4321);
        // A number that is not the one in the key, a total that does not add up, a key with a wrong digit.
        for broken in [
            base.replace("<nNF>4321</nNF>", "<nNF>4322</nNF>"),
            base.replace("<vNF>111.50</vNF>", "<vNF>111.51</vNF>"),
            base.replace("<vProd>100.00</vProd>", "<vProd>101.00</vProd>"),
            base.replace(&key_for(CNPJ, 1, 4321), &{
                let key = key_for(CNPJ, 1, 4321);
                let digit = key[43..].parse::<u32>().unwrap();
                format!("{}{}", &key[..43], (digit + 1) % 10)
            }),
        ] {
            let draft = parse_nfe(broken.as_bytes()).unwrap();
            assert!(Invoice::new(draft).is_err());
        }
    }
}
