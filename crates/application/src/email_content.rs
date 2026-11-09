/// What an e-mail says, apart from how it looks (ADR 0068). Each use case builds one and the same content is drawn as the
/// plain-text part and as the branded HTML part, so the two never say different things.
///
/// Every string is the person's or the company's own words, already one line where it must be: the HTML drawing escapes
/// them, the text drawing does not need to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmailContent {
    heading: String,
    /// The company the message comes from, shown above the heading when a company wrote it.
    company: Option<String>,
    blocks: Vec<EmailBlock>,
    /// Why the person got this message, said in the footer.
    reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum EmailBlock {
    Paragraph(String),
    /// Labelled values in a tinted box: `Valor`, `Vencimento`.
    Details(Vec<(String, String)>),
    Items(Items),
    /// A warning that must not be missed, like what to do when the person did not ask for this.
    Notice(String),
    /// Small print under the main message.
    Note(String),
    Button {
        label: String,
        url: String,
    },
}

/// A short table of lines with a quantity, a description and a price, and a total.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Items {
    pub title: String,
    pub rows: Vec<ItemRow>,
    /// The label and the amount of the total line.
    pub total: Option<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemRow {
    pub quantity: String,
    pub description: String,
    pub price: String,
}

impl EmailContent {
    pub fn new(heading: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            heading: heading.into(),
            company: None,
            blocks: Vec::new(),
            reason: reason.into(),
        }
    }

    pub fn company(mut self, name: impl Into<String>) -> Self {
        self.company = Some(name.into());
        self
    }

    pub fn paragraph(mut self, text: impl Into<String>) -> Self {
        self.blocks.push(EmailBlock::Paragraph(text.into()));
        self
    }

    pub fn details(mut self, rows: Vec<(String, String)>) -> Self {
        self.blocks.push(EmailBlock::Details(rows));
        self
    }

    pub fn items(mut self, items: Items) -> Self {
        self.blocks.push(EmailBlock::Items(items));
        self
    }

    pub fn notice(mut self, text: impl Into<String>) -> Self {
        self.blocks.push(EmailBlock::Notice(text.into()));
        self
    }

    pub fn note(mut self, text: impl Into<String>) -> Self {
        self.blocks.push(EmailBlock::Note(text.into()));
        self
    }

    pub fn button(mut self, label: impl Into<String>, url: impl Into<String>) -> Self {
        self.blocks.push(EmailBlock::Button {
            label: label.into(),
            url: url.into(),
        });
        self
    }

    /// The plain-text part: every line of the content, one block after another, a link on a line of its own.
    pub fn to_text(&self) -> String {
        let mut text = String::new();
        if let Some(company) = &self.company {
            text.push_str(company);
            text.push('\n');
        }
        text.push_str(&self.heading);
        text.push('\n');
        for block in &self.blocks {
            text.push('\n');
            match block {
                EmailBlock::Paragraph(paragraph)
                | EmailBlock::Notice(paragraph)
                | EmailBlock::Note(paragraph) => {
                    text.push_str(paragraph);
                    text.push('\n');
                }
                EmailBlock::Details(rows) => {
                    for (label, value) in rows {
                        text.push_str(&format!("{label}: {value}\n"));
                    }
                }
                EmailBlock::Items(items) => {
                    text.push_str(&format!("{}:\n", items.title));
                    for row in &items.rows {
                        text.push_str(&format!(
                            "- {} x {}: {} cada\n",
                            row.quantity, row.description, row.price
                        ));
                    }
                    if let Some((label, amount)) = &items.total {
                        text.push_str(&format!("\n{label}: {amount}\n"));
                    }
                }
                EmailBlock::Button { label, url } => text.push_str(&format!("{label}:\n{url}\n")),
            }
        }
        text.push_str(&format!("\n{}\n", self.reason));
        text
    }

    /// The HTML part, in the layout of the design canvas: a dark band with the name, a white card, and a footer
    /// outside it. Tables and inline styles only, since mail clients do not read a stylesheet; `source_url` is the
    /// link to the source code that AGPL section 13 asks for, left out when it is not an http(s) address.
    pub fn to_html(&self, source_url: &str) -> String {
        let mut body = String::new();
        if let Some(company) = &self.company {
            body.push_str(&format!(
                "<p style=\"margin:0 0 8px;font-size:13px;font-weight:600;letter-spacing:0.06em;text-transform:uppercase;color:#5d6b64\">{}</p>",
                escape(company)
            ));
        }
        body.push_str(&format!(
            "<h1 style=\"margin:0 0 16px;font-size:22px;line-height:1.3;color:#14201a\">{}</h1>",
            escape(&self.heading)
        ));
        let mut fallbacks = String::new();
        for block in &self.blocks {
            match block {
                EmailBlock::Paragraph(text) => body.push_str(&format!(
                    "<p style=\"margin:0 0 16px;font-size:15px;line-height:1.6;color:#3f4f47\">{}</p>",
                    escape(text)
                )),
                EmailBlock::Details(rows) => body.push_str(&details_html(rows)),
                EmailBlock::Items(items) => body.push_str(&items_html(items)),
                EmailBlock::Notice(text) => body.push_str(&format!(
                    "<div style=\"margin:0 0 16px;padding:12px 14px;border-radius:8px;background:#fff4d6;color:#6b4500;font-size:13px;line-height:1.5\">{}</div>",
                    escape(text)
                )),
                EmailBlock::Note(text) => body.push_str(&format!(
                    "<p style=\"margin:0 0 16px;font-size:13px;line-height:1.5;color:#5d6b64\">{}</p>",
                    escape(text)
                )),
                EmailBlock::Button { label, url } => {
                    if let Some(url) = web_address(url) {
                        body.push_str(&button_html(label, url));
                        fallbacks.push_str(&format!(
                            "<a href=\"{url}\" style=\"color:#17603c;word-break:break-all\">{url}</a><br>",
                            url = escape(url)
                        ));
                    }
                }
            }
        }
        if !fallbacks.is_empty() {
            body.push_str(&format!(
                "<div style=\"margin-top:8px;padding-top:16px;border-top:1px solid #e1e8e4;font-size:13px;line-height:1.5;color:#5d6b64\">Se o botão não funcionar, copie este endereço no navegador:<br>{fallbacks}</div>"
            ));
        }
        let source = web_address(source_url)
            .map(|url| {
                format!(
                    " <a href=\"{}\" style=\"color:#17603c\">Código-fonte</a>",
                    escape(url)
                )
            })
            .unwrap_or_default();
        format!(
            "<!doctype html><html lang=\"pt-BR\"><head><meta charset=\"utf-8\">\
             <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
             <meta name=\"color-scheme\" content=\"light\"><title>{title}</title></head>\
             <body style=\"margin:0;padding:0;background:#f5f7f6\">\
             <table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" style=\"background:#f5f7f6\"><tr>\
             <td align=\"center\" style=\"padding:24px 12px;font-family:system-ui,-apple-system,'Segoe UI',Arial,sans-serif\">\
             <table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" style=\"max-width:512px;background:#ffffff;border:1px solid #e1e8e4;border-radius:12px;overflow:hidden\">\
             <tr><td style=\"padding:20px 32px;background:#0f2a1e;font-size:18px;font-weight:700;color:#ffffff\">conta<span style=\"color:#8fd9b3\">-verde</span></td></tr>\
             <tr><td style=\"padding:32px;text-align:left\">{body}</td></tr></table>\
             <p style=\"margin:16px 0 0;max-width:512px;font-size:12px;line-height:1.5;color:#5d6b64;text-align:center\">{reason}<br>\
             conta-verde é software livre sob a GNU AGPL v3 ou posterior.{source}</p>\
             </td></tr></table></body></html>",
            title = escape(&self.heading),
            reason = escape(&self.reason),
        )
    }
}

fn button_html(label: &str, url: &str) -> String {
    format!(
        "<table role=\"presentation\" cellpadding=\"0\" cellspacing=\"0\" style=\"margin:8px 0 16px\"><tr>\
         <td bgcolor=\"#1f7a4d\" style=\"border-radius:8px;background:#1f7a4d\">\
         <a href=\"{}\" style=\"display:inline-block;padding:13px 24px;border-radius:8px;font-size:15px;font-weight:600;color:#ffffff;text-decoration:none\">{}</a>\
         </td></tr></table>",
        escape(url),
        escape(label)
    )
}

fn details_html(rows: &[(String, String)]) -> String {
    let mut html = String::from(
        "<table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" style=\"margin:0 0 16px;background:#e6f4ec;border-radius:8px\">",
    );
    for (label, value) in rows {
        html.push_str(&format!(
            "<tr><td style=\"padding:8px 14px;width:140px;font-size:13px;color:#3f4f47\">{}</td>\
             <td style=\"padding:8px 14px;font-size:14px;font-weight:600;color:#14201a\">{}</td></tr>",
            escape(label),
            escape(value)
        ));
    }
    html.push_str("</table>");
    html
}

fn items_html(items: &Items) -> String {
    let head = "padding:8px 0;border-bottom:1px solid #c9d5ce;font-size:12px;font-weight:600;letter-spacing:0.04em;text-transform:uppercase;color:#5d6b64";
    let cell = "padding:10px 0;border-bottom:1px solid #e1e8e4;font-size:14px;color:#14201a";
    let mut html = format!(
        "<table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" style=\"margin:0 0 16px\">\
         <tr><th align=\"left\" style=\"{head}\">{title}</th><th align=\"right\" style=\"{head};padding-right:8px\">Qtd.</th><th align=\"right\" style=\"{head}\">Preço</th></tr>",
        title = escape(&items.title)
    );
    for row in &items.rows {
        html.push_str(&format!(
            "<tr><td style=\"{cell}\">{}</td><td align=\"right\" style=\"{cell};padding-right:8px\">{}</td><td align=\"right\" style=\"{cell}\">{}</td></tr>",
            escape(&row.description),
            escape(&row.quantity),
            escape(&row.price)
        ));
    }
    if let Some((label, amount)) = &items.total {
        html.push_str(&format!(
            "<tr><td colspan=\"2\" align=\"right\" style=\"padding:12px 8px 0 0;font-size:14px;color:#3f4f47\">{}</td>\
             <td align=\"right\" style=\"padding:12px 0 0;font-size:18px;font-weight:700;color:#14201a\">{}</td></tr>",
            escape(label),
            escape(amount)
        ));
    }
    html.push_str("</table>");
    html
}

/// An address a link may point at: only http and https, never `javascript:` or the like.
fn web_address(url: &str) -> Option<&str> {
    let scheme = url.split_once("://")?.0;
    (scheme.eq_ignore_ascii_case("https") || scheme.eq_ignore_ascii_case("http")).then_some(url)
}

fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            other => escaped.push(other),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reminder() -> EmailContent {
        EmailContent::new("A parcela 1 vence em 3 dias", "Você recebeu este lembrete.")
            .company("Padaria <Exemplo>")
            .paragraph("Olá, Carla.")
            .details(vec![
                ("Valor".to_owned(), "R$ 100,00".to_owned()),
                ("Vencimento".to_owned(), "10/12/2026".to_owned()),
            ])
            .button("Pagar agora", "https://app.example/pay/abc")
            .note("Se você já pagou, desconsidere esta mensagem.")
    }

    #[test]
    fn the_text_says_every_line_of_the_content_with_the_link_on_its_own_line() {
        let text = reminder().to_text();
        assert_eq!(
            text,
            "Padaria <Exemplo>\nA parcela 1 vence em 3 dias\n\nOlá, Carla.\n\nValor: R$ 100,00\nVencimento: 10/12/2026\n\n\
             Pagar agora:\nhttps://app.example/pay/abc\n\nSe você já pagou, desconsidere esta mensagem.\n\nVocê recebeu este lembrete.\n"
        );
    }

    #[test]
    fn the_html_escapes_what_people_typed() {
        let html = reminder().to_html("https://example.org/source");
        assert!(html.contains("Padaria &lt;Exemplo&gt;"));
        assert!(!html.contains("<Exemplo>"));
        let hostile = EmailContent::new("<script>alert(1)</script>", "x")
            .paragraph("a \"quoted\" & <b>bold</b>");
        let html = hostile.to_html("https://example.org");
        assert!(!html.contains("<script>"));
        assert!(html.contains("a &quot;quoted&quot; &amp; &lt;b&gt;bold&lt;/b&gt;"));
    }

    #[test]
    fn the_html_has_the_button_the_link_to_copy_and_the_source_in_the_footer() {
        let html = reminder().to_html("https://example.org/source");
        assert_eq!(
            html.matches("href=\"https://app.example/pay/abc\"").count(),
            2
        );
        assert!(html.contains("Pagar agora"));
        assert!(html.contains("Se o botão não funcionar"));
        assert!(html.contains("href=\"https://example.org/source\""));
        assert!(html.contains("GNU AGPL v3"));
    }

    #[test]
    fn the_scheme_of_a_web_address_may_be_in_capitals() {
        let html = EmailContent::new("x", "y")
            .button("Abrir", "HTTPS://app.example/x")
            .to_html("HTTP://example.org");
        assert!(html.contains("HTTPS://app.example/x") && html.contains("Código-fonte"));
    }

    #[test]
    fn a_link_that_is_not_a_web_address_is_never_made() {
        let content = EmailContent::new("x", "y").button("Abrir", "javascript:alert(1)");
        let html = content.to_html("javascript:alert(2)");
        assert!(!html.contains("javascript:"));
        assert!(!html.contains("Código-fonte"));
        assert!(!html.contains("Se o botão não funcionar"));
    }

    #[test]
    fn a_table_of_items_has_its_lines_and_its_total_in_both_parts() {
        let content = EmailContent::new("Pedido de compra nº 7", "x").items(Items {
            title: "Itens".to_owned(),
            rows: vec![ItemRow {
                quantity: "10".to_owned(),
                description: "Farinha".to_owned(),
                price: "R$ 98,00".to_owned(),
            }],
            total: Some(("Total".to_owned(), "R$ 980,00".to_owned())),
        });
        assert!(
            content
                .to_text()
                .contains("Itens:\n- 10 x Farinha: R$ 98,00 cada\n\nTotal: R$ 980,00\n")
        );
        let html = content.to_html("https://example.org");
        assert!(
            html.contains("Farinha") && html.contains("R$ 98,00") && html.contains("R$ 980,00")
        );
    }
}
