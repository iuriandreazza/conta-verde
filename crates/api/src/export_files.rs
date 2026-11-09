//! The files of the accountant's export, written by hand (ADR 0071): a CSV the way the web app writes one, an XLSX of
//! five small parts and a zip that stores its files uncompressed. Nothing here reads a clock, so a package is the same
//! bytes every time it is asked for.

use conta_verde_application::{ExportCell, ExportPackage, ExportTable};

/// Which file an export is asked as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Csv,
    Xlsx,
}

impl ExportFormat {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "csv" => Some(Self::Csv),
            "xlsx" => Some(Self::Xlsx),
            _ => None,
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Xlsx => "xlsx",
        }
    }

    pub fn content_type(self) -> &'static str {
        match self {
            Self::Csv => "text/csv; charset=utf-8",
            Self::Xlsx => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        }
    }

    pub fn file_of(self, table: &ExportTable) -> Vec<u8> {
        match self {
            Self::Csv => csv_of(table),
            Self::Xlsx => xlsx_of(table),
        }
    }
}

/// A zip that cannot be written: a package is limited well below what a zip without ZIP64 holds, so this is a bug.
#[derive(Debug, PartialEq, Eq)]
pub struct ZipTooBig;

// ---- CSV ----

const BYTE_ORDER_MARK: &str = "\u{feff}";

/// A cell a spreadsheet would run as a formula: `=`, `+`, `@`, a tab or a carriage return, or a `-` that is not a plain
/// number. The same rule as `web/src/lib/csv.ts`.
fn runs_as_formula(value: &str) -> bool {
    match value.chars().next() {
        Some('=' | '+' | '@' | '\t' | '\r') => true,
        Some('-') => !is_plain_number(&value[1..]),
        _ => false,
    }
}

/// Digits, optionally a point and more digits, and nothing else.
fn is_plain_number(value: &str) -> bool {
    let (whole, fraction) = value
        .split_once('.')
        .map_or((value, None), |(w, f)| (w, Some(f)));
    let digits = |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
    digits(whole) && fraction.is_none_or(digits)
}

fn csv_cell(value: &str) -> String {
    let safe = if runs_as_formula(value) {
        format!("'{value}")
    } else {
        value.to_owned()
    };
    if safe.contains([';', '"', '\r', '\n']) {
        format!("\"{}\"", safe.replace('"', "\"\""))
    } else {
        safe
    }
}

fn cell_text(cell: &ExportCell) -> &str {
    match cell {
        ExportCell::Text(text) | ExportCell::Amount(text) => text,
    }
}

/// UTF-8 with a byte-order mark, `;` between cells and `\r\n` between lines: what Excel in Brazil opens.
pub fn csv_of(table: &ExportTable) -> Vec<u8> {
    let mut text = String::from(BYTE_ORDER_MARK);
    let header: Vec<String> = table.header.iter().map(|name| csv_cell(name)).collect();
    text.push_str(&header.join(";"));
    text.push_str("\r\n");
    for row in &table.rows {
        let cells: Vec<String> = row.iter().map(|cell| csv_cell(cell_text(cell))).collect();
        text.push_str(&cells.join(";"));
        text.push_str("\r\n");
    }
    text.into_bytes()
}

// ---- XLSX ----

const XML_HEAD: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";

/// What XML 1.0 allows in a text: tab, line feed, carriage return and everything from space up but the surrogates and
/// the two non-characters at the end of the first plane.
fn is_xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r') || (c >= ' ' && c != '\u{fffe}' && c != '\u{ffff}')
}

fn xml_text(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for c in value.chars().filter(|c| is_xml_char(*c)) {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            other => escaped.push(other),
        }
    }
    escaped
}

/// `A`, `B`… `Z`, `AA`…
fn column_letters(mut index: usize) -> String {
    let mut letters = Vec::new();
    loop {
        letters.push(b'A' + u8::try_from(index % 26).unwrap_or(0));
        if index < 26 {
            break;
        }
        index = index / 26 - 1;
    }
    letters.reverse();
    String::from_utf8(letters).unwrap_or_default()
}

fn sheet_xml(table: &ExportTable) -> String {
    let mut xml = String::from(XML_HEAD);
    xml.push_str(
        "<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData>",
    );
    let header = table
        .header
        .iter()
        .map(|name| ExportCell::Text((*name).to_owned()));
    let lines = std::iter::once(header.collect::<Vec<_>>()).chain(table.rows.iter().cloned());
    for (number, line) in lines.enumerate() {
        xml.push_str(&format!("<row r=\"{}\">", number + 1));
        for (column, cell) in line.iter().enumerate() {
            let reference = format!("{}{}", column_letters(column), number + 1);
            match cell {
                // The decimal text as it was stored: a float is never made of it.
                ExportCell::Amount(text) => xml.push_str(&format!(
                    "<c r=\"{reference}\"><v>{}</v></c>",
                    xml_text(text)
                )),
                ExportCell::Text(text) => xml.push_str(&format!(
                    "<c r=\"{reference}\" t=\"inlineStr\"><is><t xml:space=\"preserve\">{}</t></is></c>",
                    xml_text(text)
                )),
            }
        }
        xml.push_str("</row>");
    }
    xml.push_str("</sheetData></worksheet>");
    xml
}

/// An XLSX is a zip of five parts; the same columns as the CSV, amounts as numbers and everything else as text.
pub fn xlsx_of(table: &ExportTable) -> Vec<u8> {
    let content_types = format!(
        "{XML_HEAD}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
         <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
         <Default Extension=\"xml\" ContentType=\"application/xml\"/>\
         <Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/>\
         <Override PartName=\"/xl/worksheets/sheet1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>\
         </Types>"
    );
    let root_rels = format!(
        "{XML_HEAD}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
         <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/>\
         </Relationships>"
    );
    let workbook = format!(
        "{XML_HEAD}<workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" \
         xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">\
         <sheets><sheet name=\"{}\" sheetId=\"1\" r:id=\"rId1\"/></sheets></workbook>",
        xml_text(table.name)
    );
    let workbook_rels = format!(
        "{XML_HEAD}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
         <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet1.xml\"/>\
         </Relationships>"
    );
    let parts = [
        ("[Content_Types].xml", content_types.into_bytes()),
        ("_rels/.rels", root_rels.into_bytes()),
        ("xl/workbook.xml", workbook.into_bytes()),
        ("xl/_rels/workbook.xml.rels", workbook_rels.into_bytes()),
        ("xl/worksheets/sheet1.xml", sheet_xml(table).into_bytes()),
    ];
    // Five small parts are far below what a zip without ZIP64 holds, unless a sheet is gigabytes: the row limit
    // keeps it at some megabytes.
    zip_of(&parts).unwrap_or_default()
}

// ---- ZIP ----

/// The CRC-32 of a zip (the reflected polynomial `0xEDB88320`).
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFF_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// 1980-01-01 00:00:00: the server reads no clock, and a package is the same bytes every time.
const DOS_TIME: u16 = 0;
const DOS_DATE: u16 = (1 << 5) | 1;
/// Names are UTF-8.
const UTF8_FLAG: u16 = 1 << 11;

fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// A zip that stores each file as it is: local headers and data, the central directory, the end record. No ZIP64:
/// at most 65,535 files of less than 4 GiB, far above the limits of a package.
pub fn zip_of<N: AsRef<str>, D: AsRef<[u8]>>(files: &[(N, D)]) -> Result<Vec<u8>, ZipTooBig> {
    let count = u16::try_from(files.len()).map_err(|_| ZipTooBig)?;
    let mut out = Vec::new();
    // (name, crc, size, offset of the local header)
    let mut directory: Vec<(&str, u32, u32, u32)> = Vec::new();
    for (name, data) in files {
        let (name, data) = (name.as_ref(), data.as_ref());
        let offset = u32::try_from(out.len()).map_err(|_| ZipTooBig)?;
        let size = u32::try_from(data.len()).map_err(|_| ZipTooBig)?;
        let name_length = u16::try_from(name.len()).map_err(|_| ZipTooBig)?;
        let crc = crc32(data);
        push_u32(&mut out, 0x0403_4B50);
        push_u16(&mut out, 20);
        push_u16(&mut out, UTF8_FLAG);
        push_u16(&mut out, 0);
        push_u16(&mut out, DOS_TIME);
        push_u16(&mut out, DOS_DATE);
        push_u32(&mut out, crc);
        push_u32(&mut out, size);
        push_u32(&mut out, size);
        push_u16(&mut out, name_length);
        push_u16(&mut out, 0);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
        directory.push((name, crc, size, offset));
    }
    let directory_start = u32::try_from(out.len()).map_err(|_| ZipTooBig)?;
    for (name, crc, size, offset) in &directory {
        push_u32(&mut out, 0x0201_4B50);
        push_u16(&mut out, 20);
        push_u16(&mut out, 20);
        push_u16(&mut out, UTF8_FLAG);
        push_u16(&mut out, 0);
        push_u16(&mut out, DOS_TIME);
        push_u16(&mut out, DOS_DATE);
        push_u32(&mut out, *crc);
        push_u32(&mut out, *size);
        push_u32(&mut out, *size);
        push_u16(&mut out, u16::try_from(name.len()).map_err(|_| ZipTooBig)?);
        push_u16(&mut out, 0);
        push_u16(&mut out, 0);
        push_u16(&mut out, 0);
        push_u16(&mut out, 0);
        push_u32(&mut out, 0);
        push_u32(&mut out, *offset);
        out.extend_from_slice(name.as_bytes());
    }
    let directory_size = u32::try_from(out.len())
        .map_err(|_| ZipTooBig)?
        .checked_sub(directory_start)
        .ok_or(ZipTooBig)?;
    push_u32(&mut out, 0x0605_4B50);
    push_u16(&mut out, 0);
    push_u16(&mut out, 0);
    push_u16(&mut out, count);
    push_u16(&mut out, count);
    push_u32(&mut out, directory_size);
    push_u32(&mut out, directory_start);
    push_u16(&mut out, 0);
    Ok(out)
}

/// The month's package: the three tables in the chosen format, the list of attachments and the attached files. The
/// package is taken by value so the attached files (up to 200 MiB) are moved into the zip, not copied.
pub fn package_zip(package: ExportPackage, format: ExportFormat) -> Result<Vec<u8>, ZipTooBig> {
    let extension = format.extension();
    let mut files: Vec<(String, Vec<u8>)> = vec![
        (
            format!("entries.{extension}"),
            format.file_of(&package.entries),
        ),
        (
            format!("settlements.{extension}"),
            format.file_of(&package.settlements),
        ),
        (
            format!("transfers.{extension}"),
            format.file_of(&package.transfers),
        ),
        // The list of the attached files is always a CSV: it is read by people more than by software.
        ("attachments.csv".to_owned(), csv_of(&package.attachments)),
    ];
    files.extend(
        package
            .files
            .into_iter()
            .map(|file| (file.path, file.bytes)),
    );
    zip_of(&files)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn table(rows: Vec<Vec<ExportCell>>) -> ExportTable {
        ExportTable {
            name: "entries",
            header: &["name", "amount"],
            rows,
        }
    }

    fn text(value: &str) -> ExportCell {
        ExportCell::Text(value.to_owned())
    }

    fn amount(value: &str) -> ExportCell {
        ExportCell::Amount(value.to_owned())
    }

    /// Reads a zip back the way any reader does: the end record, then the central directory, then the data.
    pub(crate) fn unzip(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
        let u16_at = |at: usize| usize::from(u16::from_le_bytes([bytes[at], bytes[at + 1]]));
        let u32_at = |at: usize| {
            u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]) as usize
        };
        let end = bytes.len() - 22;
        assert_eq!(u32_at(end), 0x0605_4B50, "the end record closes the file");
        let (count, start) = (u16_at(end + 10), u32_at(end + 16));
        let mut at = start;
        let mut files = Vec::new();
        for _ in 0..count {
            assert_eq!(u32_at(at), 0x0201_4B50);
            assert_eq!(
                u16_at(at + 8) & (1 << 11),
                1 << 11,
                "names are flagged UTF-8"
            );
            assert_eq!(u16_at(at + 10), 0, "stored, not compressed");
            let (crc, size) = (u32_at(at + 16) as u32, u32_at(at + 24));
            let (name_length, offset) = (u16_at(at + 28), u32_at(at + 42));
            let name = String::from_utf8(bytes[at + 46..at + 46 + name_length].to_vec()).unwrap();
            assert_eq!(u32_at(offset), 0x0403_4B50);
            let local_name = u16_at(offset + 26);
            let data_start = offset + 30 + local_name + u16_at(offset + 28);
            let data = bytes[data_start..data_start + size].to_vec();
            assert_eq!(crc32(&data), crc, "the checksum is of the data");
            files.push((name, data));
            at += 46 + name_length;
        }
        assert_eq!(at, end, "the directory ends where the end record starts");
        files
    }

    #[test]
    fn the_crc_of_the_standard_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn a_csv_has_a_mark_semicolons_and_crlf_lines() {
        let csv = csv_of(&table(vec![vec![text("Aluguel"), amount("1200.5")]]));
        assert_eq!(
            String::from_utf8(csv).unwrap(),
            "\u{feff}name;amount\r\nAluguel;1200.5\r\n"
        );
    }

    #[test]
    fn a_csv_quotes_what_would_break_a_line_and_defuses_formulas() {
        let csv = String::from_utf8(csv_of(&table(vec![
            vec![text("a;b \"c\"\nd"), amount("1")],
            vec![text("=SUM(A1)"), text("+1")],
            vec![text("@x"), text("-1")],
            vec![text("-10.5"), text("-text")],
            vec![text("\tx"), text("-")],
        ])))
        .unwrap();
        assert_eq!(
            csv,
            "\u{feff}name;amount\r\n\
             \"a;b \"\"c\"\"\nd\";1\r\n\
             '=SUM(A1);'+1\r\n\
             '@x;-1\r\n\
             -10.5;'-text\r\n\
             '\tx;'-\r\n"
        );
    }

    #[test]
    fn a_plain_number_is_digits_with_one_optional_fraction() {
        for yes in ["1", "10", "10.5", "0.001"] {
            assert!(is_plain_number(yes), "{yes}");
        }
        for no in ["", ".5", "5.", "1.2.3", "1e3", "1,5", "a"] {
            assert!(!is_plain_number(no), "{no}");
        }
    }

    #[test]
    fn column_letters_run_past_z() {
        assert_eq!(column_letters(0), "A");
        assert_eq!(column_letters(15), "P");
        assert_eq!(column_letters(25), "Z");
        assert_eq!(column_letters(26), "AA");
        assert_eq!(column_letters(27), "AB");
        assert_eq!(column_letters(701), "ZZ");
        assert_eq!(column_letters(702), "AAA");
    }

    #[test]
    fn a_zip_stores_each_file_with_its_checksum_and_a_fixed_day() {
        let bytes = zip_of(&[("a.txt", b"hello".to_vec()), ("dir/ç.txt", b"".to_vec())]).unwrap();
        let files = unzip(&bytes);
        assert_eq!(
            files,
            vec![
                ("a.txt".to_owned(), b"hello".to_vec()),
                ("dir/ç.txt".to_owned(), Vec::new())
            ]
        );
        // 1980-01-01 in the first local header, so the same files are the same bytes tomorrow.
        assert_eq!(&bytes[10..14], &[0, 0, 0x21, 0]);
        assert_eq!(
            bytes,
            zip_of(&[("a.txt", b"hello".to_vec()), ("dir/ç.txt", b"".to_vec())]).unwrap()
        );
    }

    #[test]
    fn a_zip_of_nothing_is_just_the_end_record() {
        let none: [(&str, &[u8]); 0] = [];
        let bytes = zip_of(&none).unwrap();
        assert_eq!(bytes.len(), 22);
        assert!(unzip(&bytes).is_empty());
    }

    #[test]
    fn an_xlsx_is_five_parts_with_text_inline_and_amounts_as_numbers() {
        let bytes = xlsx_of(&table(vec![vec![
            text("A & <B>\u{0}\u{b}"),
            amount("1200.50"),
        ]]));
        let parts = unzip(&bytes);
        let names: Vec<&str> = parts.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(
            names,
            [
                "[Content_Types].xml",
                "_rels/.rels",
                "xl/workbook.xml",
                "xl/_rels/workbook.xml.rels",
                "xl/worksheets/sheet1.xml"
            ]
        );
        let sheet = String::from_utf8(parts[4].1.clone()).unwrap();
        assert!(sheet.contains(
            "<row r=\"1\"><c r=\"A1\" t=\"inlineStr\"><is><t xml:space=\"preserve\">name</t></is></c>"
        ));
        // Escaped, and the characters XML 1.0 forbids are gone.
        assert!(sheet.contains("<t xml:space=\"preserve\">A &amp; &lt;B&gt;</t>"));
        // The decimal text as stored, in a numeric cell.
        assert!(sheet.contains("<c r=\"B2\"><v>1200.50</v></c>"));
        let workbook = String::from_utf8(parts[2].1.clone()).unwrap();
        assert!(workbook.contains("<sheet name=\"entries\" sheetId=\"1\" r:id=\"rId1\"/>"));
    }
}
