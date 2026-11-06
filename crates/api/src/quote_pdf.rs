//! A quote as a PDF, written by hand: a few lines of text in two standard fonts need no library, and
//! the file is a document a person opens, not one anything reads back ([ADR 0038]).

const PAGE_WIDTH: u32 = 595;
const PAGE_HEIGHT: u32 = 842;
const MARGIN: u32 = 50;
const LINE_HEIGHT: u32 = 15;
/// Where a page stops being written on and the next begins.
const BOTTOM: u32 = 70;
const NOTES_WIDTH: usize = 95;

/// One line of the quote as it is printed: every figure already written the way a person reads it.
pub(crate) struct PdfLine {
    pub description: String,
    pub quantity: String,
    pub unit_price: String,
    pub discount: String,
    pub total: String,
}

pub(crate) struct PdfQuote {
    pub company: String,
    pub customer: String,
    pub number: i64,
    pub issued_on: String,
    pub valid_until: String,
    pub payment_term: Option<String>,
    pub notes: Option<String>,
    pub lines: Vec<PdfLine>,
    pub total: String,
}

/// `45.1` as `45,10` and `1234567.8` as `1.234.567,80`: how money is written in Brazil, from the
/// decimal text the API uses, without going through a number.
pub(crate) fn brl(decimal: &str) -> String {
    let (integer, fraction) = decimal.split_once('.').unwrap_or((decimal, ""));
    let mut grouped = String::new();
    for (index, digit) in integer.chars().enumerate() {
        if index > 0 && (integer.len() - index).is_multiple_of(3) {
            grouped.push('.');
        }
        grouped.push(digit);
    }
    format!("{grouped},{fraction:0<2}")
}

/// A quantity with the decimal comma and no zeros that mean nothing.
pub(crate) fn quantity(decimal: &str) -> String {
    decimal.replace('.', ",")
}

/// The bytes of a text in the encoding of the standard fonts (Windows-1252, whose first 256 values
/// are Latin-1 for what Portuguese needs); what it has no byte for becomes a question mark. The
/// characters that mean something inside a PDF string are escaped.
fn encode(text: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(text.len());
    for c in text.chars() {
        match u8::try_from(u32::from(c)) {
            Ok(b @ (b'(' | b')' | b'\\')) => {
                bytes.push(b'\\');
                bytes.push(b);
            }
            Ok(b) if !c.is_control() => bytes.push(b),
            _ => bytes.push(b'?'),
        }
    }
    bytes
}

/// The text cut to fit a column, with an ellipsis when it did not.
fn clipped(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_owned();
    }
    let kept: String = text.chars().take(width.saturating_sub(1)).collect();
    format!("{kept}…")
}

/// The text broken into lines of at most `width` characters at spaces; a word longer than a line is cut.
fn wrapped(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.lines() {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let word: String = word.chars().take(width).collect();
            let needed =
                line.chars().count() + usize::from(!line.is_empty()) + word.chars().count();
            if needed > width {
                lines.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(&word);
        }
        lines.push(line);
    }
    lines
}

/// The pages being written: the content streams, and where on the current one the next line goes.
struct Pages {
    streams: Vec<Vec<u8>>,
    y: u32,
}

impl Pages {
    fn new() -> Self {
        Self {
            streams: vec![Vec::new()],
            y: PAGE_HEIGHT - MARGIN,
        }
    }

    fn text(&mut self, x: u32, size: u32, bold: bool, text: &str) {
        let stream = self.streams.last_mut().expect("there is always a page");
        stream.extend_from_slice(
            format!(
                "BT /{} {size} Tf {x} {} Td (",
                if bold { "F2" } else { "F1" },
                self.y
            )
            .as_bytes(),
        );
        stream.extend_from_slice(&encode(text));
        stream.extend_from_slice(b") Tj ET\n");
    }

    /// Moves down one line, onto a new page when this one is full.
    fn next_line(&mut self) {
        if self.y < BOTTOM + LINE_HEIGHT {
            self.streams.push(Vec::new());
            self.y = PAGE_HEIGHT - MARGIN;
        } else {
            self.y -= LINE_HEIGHT;
        }
    }

    fn gap(&mut self) {
        self.y = self.y.saturating_sub(LINE_HEIGHT / 2);
    }

    fn rule(&mut self) {
        let stream = self.streams.last_mut().expect("there is always a page");
        stream.extend_from_slice(
            format!(
                "0.6 w {MARGIN} {} m {} {} l S\n",
                self.y + 11,
                PAGE_WIDTH - MARGIN,
                self.y + 11
            )
            .as_bytes(),
        );
    }
}

pub(crate) fn render(quote: &PdfQuote) -> Vec<u8> {
    let mut pages = Pages::new();
    pages.text(MARGIN, 18, true, &format!("Orçamento nº {}", quote.number));
    pages.next_line();
    pages.next_line();
    pages.text(MARGIN, 11, true, &quote.company);
    pages.next_line();
    pages.text(MARGIN, 11, false, &format!("Cliente: {}", quote.customer));
    pages.next_line();
    pages.text(
        MARGIN,
        11,
        false,
        &format!(
            "Emitido em {}   Válido até {}",
            date(&quote.issued_on),
            date(&quote.valid_until)
        ),
    );
    if let Some(term) = &quote.payment_term {
        pages.next_line();
        pages.text(MARGIN, 11, false, &format!("Condição de pagamento: {term}"));
    }
    pages.next_line();
    pages.next_line();
    pages.text(MARGIN, 10, true, "Descrição");
    pages.text(300, 10, true, "Qtd.");
    pages.text(345, 10, true, "Preço");
    pages.text(420, 10, true, "Desconto");
    pages.text(490, 10, true, "Total");
    pages.rule();
    for line in &quote.lines {
        pages.next_line();
        pages.text(MARGIN, 10, false, &clipped(&line.description, 48));
        pages.text(300, 10, false, &clipped(&line.quantity, 9));
        pages.text(345, 10, false, &clipped(&line.unit_price, 12));
        pages.text(420, 10, false, &clipped(&line.discount, 12));
        pages.text(490, 10, false, &clipped(&line.total, 14));
    }
    pages.next_line();
    pages.rule();
    pages.gap();
    pages.next_line();
    pages.text(420, 11, true, "Total");
    pages.text(490, 11, true, &format!("R$ {}", quote.total));
    if let Some(notes) = &quote.notes {
        pages.next_line();
        pages.next_line();
        pages.text(MARGIN, 10, true, "Observações");
        for line in wrapped(notes, NOTES_WIDTH) {
            pages.next_line();
            pages.text(MARGIN, 10, false, &line);
        }
    }
    document(&pages.streams)
}

/// `2026-11-06` as `06/11/2026`.
fn date(day: &str) -> String {
    match day.split('-').collect::<Vec<_>>()[..] {
        [year, month, day] => format!("{day}/{month}/{year}"),
        _ => day.to_owned(),
    }
}

/// The objects of the file: the catalog, the page tree, two fonts, and a page and its content for
/// each stream, followed by the table that says where each begins.
fn document(streams: &[Vec<u8>]) -> Vec<u8> {
    let first_page = 5;
    let page_ids: Vec<usize> = (0..streams.len()).map(|n| first_page + 2 * n).collect();
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    let mut object = |out: &mut Vec<u8>, body: Vec<u8>| {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", offsets.len()).as_bytes());
        out.extend_from_slice(&body);
        out.extend_from_slice(b"\nendobj\n");
    };
    object(&mut out, b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    let kids: Vec<String> = page_ids.iter().map(|id| format!("{id} 0 R")).collect();
    object(
        &mut out,
        format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            kids.join(" "),
            streams.len()
        )
        .into_bytes(),
    );
    for font in ["Helvetica", "Helvetica-Bold"] {
        object(
            &mut out,
            format!(
                "<< /Type /Font /Subtype /Type1 /BaseFont /{font} /Encoding /WinAnsiEncoding >>"
            )
            .into_bytes(),
        );
    }
    for (page_id, stream) in page_ids.iter().zip(streams) {
        object(
            &mut out,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_WIDTH} {PAGE_HEIGHT}] \
                 /Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> /Contents {} 0 R >>",
                page_id + 1
            )
            .into_bytes(),
        );
        let mut body = format!("<< /Length {} >>\nstream\n", stream.len()).into_bytes();
        body.extend_from_slice(stream);
        body.extend_from_slice(b"endstream");
        object(&mut out, body);
    }
    let table_at = out.len();
    out.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1).as_bytes(),
    );
    for offset in &offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{table_at}\n%%EOF\n",
            offsets.len() + 1
        )
        .as_bytes(),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quote(lines: usize) -> PdfQuote {
        PdfQuote {
            company: "Padaria (Central) \\ Ltda".into(),
            customer: "João da Conceição".into(),
            number: 7,
            issued_on: "2026-11-06".into(),
            valid_until: "2026-11-20".into(),
            payment_term: Some("30/60".into()),
            notes: Some("Entrega em cinco dias úteis.\nPagamento por Pix.".into()),
            lines: (0..lines)
                .map(|n| PdfLine {
                    description: format!("Café nº {n}"),
                    quantity: "1,5".into(),
                    unit_price: "10,10".into(),
                    discount: "0,00".into(),
                    total: "15,15".into(),
                })
                .collect(),
            total: brl("1234.5"),
        }
    }

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack
            .windows(needle.len())
            .any(|window| window == needle)
    }

    #[test]
    fn writes_money_the_brazilian_way() {
        assert_eq!(brl("45.1"), "45,10");
        assert_eq!(brl("0"), "0,00");
        assert_eq!(brl("1234567.89"), "1.234.567,89");
        assert_eq!(brl("999"), "999,00");
        assert_eq!(brl("1000"), "1.000,00");
        assert_eq!(quantity("1.5"), "1,5");
    }

    #[test]
    fn encodes_accents_and_escapes_what_a_pdf_string_reserves() {
        assert_eq!(encode("Café"), [b'C', b'a', b'f', 0xE9]);
        assert_eq!(encode("a(b)c\\"), b"a\\(b\\)c\\\\");
        // Not in the font's encoding: a question mark, never a broken file.
        assert_eq!(encode("€ ok"), b"? ok");
        assert_eq!(encode("a\nb"), b"a?b");
    }

    #[test]
    fn the_table_points_at_every_object_and_the_file_ends_properly() {
        for lines in [1, 80] {
            let pdf = render(&quote(lines));
            assert!(pdf.starts_with(b"%PDF-1.4\n"));
            assert!(pdf.ends_with(b"%%EOF\n"));
            // Offsets are in bytes of the file, and the text has Latin-1 in it: read only the tail as text.
            let tail = String::from_utf8_lossy(&pdf[pdf.len() - 40..]).into_owned();
            let start: usize = tail
                .rsplit("startxref\n")
                .next()
                .unwrap()
                .lines()
                .next()
                .unwrap()
                .parse()
                .unwrap();
            assert!(pdf[start..].starts_with(b"xref\n"));
            let text = String::from_utf8_lossy(&pdf[start..]).into_owned();
            let table: Vec<&str> = text.lines().skip(3).collect();
            let mut found = 0;
            for (index, entry) in table.iter().take_while(|l| l.ends_with(" n ")).enumerate() {
                let offset: usize = entry[..10].parse().unwrap();
                assert!(
                    pdf[offset..].starts_with(format!("{} 0 obj\n", index + 1).as_bytes()),
                    "object {} at {offset}",
                    index + 1
                );
                found += 1;
            }
            // Four fixed objects and a page with its content for each page: eighty lines need two pages.
            let pages = if lines == 80 { 2 } else { 1 };
            assert_eq!(found, 4 + 2 * pages);
            assert!(contains(&pdf, format!("/Count {pages}").as_bytes()));
        }
    }

    #[test]
    fn prints_the_figures_in_their_encoding() {
        let pdf = render(&quote(1));
        assert!(contains(&pdf, b"Or\xE7amento n\xBA 7"));
        assert!(contains(&pdf, b"Jo\xE3o da Concei\xE7\xE3o"));
        assert!(contains(&pdf, b"Padaria \\(Central\\) \\\\ Ltda"));
        assert!(contains(&pdf, b"06/11/2026"));
        assert!(contains(&pdf, b"R$ 1.234,50"));
        assert!(contains(&pdf, b"Pagamento por Pix."));
    }

    #[test]
    fn clips_and_wraps_long_text() {
        assert_eq!(clipped("abcdef", 4), "abc…");
        assert_eq!(clipped("abc", 4), "abc");
        assert_eq!(wrapped("um dois três", 8), ["um dois", "três"]);
        assert_eq!(wrapped("a\n\nb", 8), ["a", "", "b"]);
        assert_eq!(wrapped("abcdefghij", 4), ["abcd"]);
    }
}
