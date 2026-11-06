use conta_verde_domain::StatementLineDraft;

/// The most characters of a description a line keeps.
const MAX_DESCRIPTION_CHARS: usize = 200;

/// Why a statement file is refused. Each message says what to change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OfxError {
    /// Not an OFX file at all.
    NotOfx,
    /// An OFX file with no transactions in it.
    NoTransactions,
    /// A transaction that cannot be read: the number counts them from 1.
    Unreadable {
        transaction: usize,
        reason: &'static str,
    },
}

impl std::fmt::Display for OfxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotOfx => f.write_str("the file is not an OFX statement"),
            Self::NoTransactions => f.write_str("the statement has no transactions"),
            Self::Unreadable {
                transaction,
                reason,
            } => {
                write!(f, "transaction {transaction}: {reason}")
            }
        }
    }
}

/// The text of a statement file: UTF-8 when it is, and otherwise the Windows-1252 that Brazilian banks still
/// send (`CHARSET:1252`): Latin-1, except that bytes 0x80 to 0x9F are the punctuation of the Windows
/// code page (the euro, quotation marks, dashes) and not control characters.
pub fn decode(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_owned(),
        Err(_) => bytes.iter().map(|byte| windows_1252(*byte)).collect(),
    }
}

fn windows_1252(byte: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž',
        '\u{8f}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}',
        'ž', 'Ÿ',
    ];
    match byte {
        0x80..=0x9f => HIGH[usize::from(byte - 0x80)],
        other => char::from(other),
    }
}

/// The lines of an OFX statement, in the order of the file: one per `STMTTRN`, with the day it was posted
/// (`DTPOSTED`), the signed amount (`TRNAMT`), the bank's reference (`FITID`) and the description
/// (`MEMO`, or `NAME` when there is none, and both when they differ). It reads OFX 1.x, where tags are not
/// closed, and 2.x, which is XML, the same way: a value is the text up to the next tag.
pub fn parse_ofx(text: &str) -> Result<Vec<StatementLineDraft>, OfxError> {
    let upper = text.to_ascii_uppercase();
    if !upper.contains("<OFX>") && !upper.contains("OFXHEADER") {
        return Err(OfxError::NotOfx);
    }
    let mut lines = Vec::new();
    let mut from = 0;
    while let Some(start) = upper[from..].find("<STMTTRN>") {
        let body_start = from + start + "<STMTTRN>".len();
        // The next transaction bounds the search for the closing tag: a file with no closing tags is read
        // in one pass, not by a search to its end for every transaction.
        let next = upper[body_start..]
            .find("<STMTTRN>")
            .map_or(text.len(), |offset| body_start + offset);
        let body_end = upper[body_start..next]
            .find("</STMTTRN>")
            .map_or(next, |offset| body_start + offset);
        lines.push(transaction(&text[body_start..body_end], lines.len() + 1)?);
        from = body_end;
    }
    if lines.is_empty() {
        return Err(OfxError::NoTransactions);
    }
    Ok(lines)
}

fn transaction(body: &str, number: usize) -> Result<StatementLineDraft, OfxError> {
    let unreadable = |reason| OfxError::Unreadable {
        transaction: number,
        reason,
    };
    let value = |tag: &str| value_of(body, tag);
    let posted = value("DTPOSTED").ok_or_else(|| unreadable("it has no DTPOSTED"))?;
    let amount = value("TRNAMT").ok_or_else(|| unreadable("it has no TRNAMT"))?;
    let memo = value("MEMO");
    let name = value("NAME");
    let description = match (memo, name) {
        (Some(memo), Some(name)) if memo != name => format!("{name} {memo}"),
        (Some(memo), _) => memo,
        (None, Some(name)) => name,
        (None, None) => value("TRNTYPE").unwrap_or_default(),
    };
    // OFX allows 255 characters for a memo and a line keeps 200: the rest of a long one is cut, not
    // a reason to refuse the file.
    let description: String = description.chars().take(MAX_DESCRIPTION_CHARS).collect();
    Ok(StatementLineDraft {
        posted_on: day_of(&posted).ok_or_else(|| unreadable("DTPOSTED is not a date"))?,
        amount: decimal_of(&amount).ok_or_else(|| unreadable("TRNAMT is not a number"))?,
        description,
        bank_ref: value("FITID"),
    })
}

/// The text after `<TAG>` up to the next tag, trimmed, when there is some.
fn value_of(body: &str, tag: &str) -> Option<String> {
    let upper = body.to_ascii_uppercase();
    let open = format!("<{tag}>");
    let start = upper.find(&open)? + open.len();
    let end = body[start..]
        .find('<')
        .map_or(body.len(), |offset| start + offset);
    let text = unescape(body[start..end].trim());
    (!text.is_empty()).then_some(text)
}

/// The five entities XML (OFX 2.x) writes in a value; `&amp;` last, so that `&amp;lt;` stays `&lt;`.
fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// `YYYYMMDD`, optionally followed by a time and a zone (`20261105120000[-3:BRT]`), as `YYYY-MM-DD`: the
/// day the bank wrote, never moved by the zone.
fn day_of(raw: &str) -> Option<String> {
    let digits = raw.get(..8)?;
    digits
        .bytes()
        .all(|byte| byte.is_ascii_digit())
        .then(|| format!("{}-{}-{}", &digits[..4], &digits[4..6], &digits[6..8]))
}

/// The amount as the plain decimal the domain reads: a dot for the decimals, and a comma accepted when a
/// bank writes it, never with thousands separators.
fn decimal_of(raw: &str) -> Option<String> {
    let text = raw.trim().replace(',', ".");
    let unsigned = text.strip_prefix(['-', '+']).unwrap_or(&text);
    // Three decimals would be a thousands separator (`1.000`, `1,000`), which is not what a bank means by
    // an amount in cents and must not be read as 1.
    let decimals_fit = unsigned
        .split_once('.')
        .is_none_or(|(_, decimals)| decimals.len() <= 2);
    let valid = !unsigned.is_empty()
        && unsigned.matches('.').count() <= 1
        && decimals_fit
        && unsigned
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.');
    valid.then(|| text.strip_prefix('+').unwrap_or(&text).to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SGML: &str = "OFXHEADER:100\nDATA:OFXSGML\nVERSION:102\nCHARSET:1252\n\n<OFX>\n<BANKMSGSRSV1><STMTTRNRS><STMTRS>\n<CURDEF>BRL\n<BANKTRANLIST>\n<DTSTART>20261101\n<DTEND>20261130\n\
<STMTTRN>\n<TRNTYPE>CREDIT\n<DTPOSTED>20261105120000[-3:BRT]\n<TRNAMT>1250.50\n<FITID>0001\n<MEMO>PIX RECEBIDO Maria\n</STMTTRN>\n\
<STMTTRN>\n<TRNTYPE>DEBIT\n<DTPOSTED>20261106\n<TRNAMT>-30.00\n<FITID>0002\n<NAME>TARIFA\n<MEMO>PACOTE DE SERVICOS\n</STMTTRN>\n\
<STMTTRN>\n<TRNTYPE>DEBIT\n<DTPOSTED>20261107\n<TRNAMT>-5,5\n<MEMO>CAFE\n\
<STMTTRN>\n<TRNTYPE>DEBIT\n<DTPOSTED>20261107\n<TRNAMT>-5,5\n<MEMO>CAFE\n\
</BANKTRANLIST></STMTRS></STMTTRNRS></BANKMSGSRSV1></OFX>";

    #[test]
    fn reads_the_transactions_of_an_sgml_statement() {
        let lines = parse_ofx(SGML).unwrap();
        assert_eq!(lines.len(), 4);
        assert_eq!(
            (
                lines[0].posted_on.as_str(),
                lines[0].amount.as_str(),
                lines[0].description.as_str(),
                lines[0].bank_ref.as_deref()
            ),
            ("2026-11-05", "1250.50", "PIX RECEBIDO Maria", Some("0001"))
        );
        // Name and memo together when they differ; a comma for the decimals; no reference.
        assert_eq!(lines[1].description, "TARIFA PACOTE DE SERVICOS");
        assert_eq!(lines[1].amount, "-30.00");
        assert_eq!(
            (lines[2].amount.as_str(), lines[2].bank_ref.as_deref()),
            ("-5.5", None)
        );
        // A transaction that is not closed ends where the next one starts.
        assert_eq!(lines[3].description, "CAFE");
    }

    #[test]
    fn reads_an_xml_statement_the_same_way() {
        let xml = "<?xml version=\"1.0\"?><OFX><BANKMSGSRSV1><STMTTRNRS><STMTRS><BANKTRANLIST>\
<STMTTRN><TRNTYPE>DEBIT</TRNTYPE><DTPOSTED>20261105</DTPOSTED><TRNAMT>-12.34</TRNAMT><FITID>X1</FITID><MEMO>Luz</MEMO></STMTTRN>\
</BANKTRANLIST></STMTRS></STMTTRNRS></BANKMSGSRSV1></OFX>";
        let lines = parse_ofx(xml).unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(
            (
                lines[0].posted_on.as_str(),
                lines[0].amount.as_str(),
                lines[0].description.as_str(),
                lines[0].bank_ref.as_deref()
            ),
            ("2026-11-05", "-12.34", "Luz", Some("X1"))
        );
    }

    #[test]
    fn refuses_what_is_not_a_statement_or_has_nothing_or_cannot_be_read() {
        assert_eq!(parse_ofx("a;b;c\n1;2;3"), Err(OfxError::NotOfx));
        assert_eq!(parse_ofx("<OFX></OFX>"), Err(OfxError::NoTransactions));
        let broken =
            |inner: &str| parse_ofx(&format!("<OFX><STMTTRN>{inner}</STMTTRN></OFX>")).unwrap_err();
        assert_eq!(
            broken("<TRNAMT>1<MEMO>x"),
            OfxError::Unreadable {
                transaction: 1,
                reason: "it has no DTPOSTED"
            }
        );
        assert_eq!(
            broken("<DTPOSTED>20261105<MEMO>x"),
            OfxError::Unreadable {
                transaction: 1,
                reason: "it has no TRNAMT"
            }
        );
        assert_eq!(
            broken("<DTPOSTED>hoje<TRNAMT>1"),
            OfxError::Unreadable {
                transaction: 1,
                reason: "DTPOSTED is not a date"
            }
        );
        assert_eq!(
            broken("<DTPOSTED>20261105<TRNAMT>1.234,50"),
            OfxError::Unreadable {
                transaction: 1,
                reason: "TRNAMT is not a number"
            }
        );
    }

    #[test]
    fn a_thousands_separator_is_not_an_amount_in_cents() {
        let amount = |raw: &str| {
            parse_ofx(&format!(
                "<OFX><STMTTRN><DTPOSTED>20261105<TRNAMT>{raw}<MEMO>x</STMTTRN></OFX>"
            ))
        };
        assert_eq!(amount("1.50").unwrap()[0].amount, "1.50");
        assert_eq!(amount("-1,5").unwrap()[0].amount, "-1.5");
        for raw in ["1.000", "1,000", "1.234,50", "1,234.50"] {
            assert_eq!(
                amount(raw),
                Err(OfxError::Unreadable {
                    transaction: 1,
                    reason: "TRNAMT is not a number"
                }),
                "{raw}"
            );
        }
    }

    #[test]
    fn keeps_a_long_description_cut_and_reads_windows_1252_punctuation() {
        let long = format!(
            "<OFX><STMTTRN><DTPOSTED>20261105<TRNAMT>-1<MEMO>{}</STMTTRN></OFX>",
            "x".repeat(255)
        );
        assert_eq!(
            parse_ofx(&long).unwrap()[0].description.chars().count(),
            200
        );
        assert_eq!(
            decode(b"Pagto \x96 Cia \x93ok\x94 \x80"),
            "Pagto – Cia “ok” €"
        );
    }

    #[test]
    fn reads_the_entities_of_an_xml_value() {
        let xml = "<OFX><STMTTRN><DTPOSTED>20261105</DTPOSTED><TRNAMT>-1</TRNAMT><MEMO>Bar &amp; Cia &lt;centro&gt; &amp;lt;</MEMO></STMTTRN></OFX>";
        assert_eq!(
            parse_ofx(xml).unwrap()[0].description,
            "Bar & Cia <centro> &lt;"
        );
    }

    #[test]
    fn decodes_latin_1_when_the_file_is_not_utf_8() {
        assert_eq!(decode("Pagamento ação".as_bytes()), "Pagamento ação");
        assert_eq!(decode(b"Pagamento a\xe7\xe3o"), "Pagamento ação");
    }
}
