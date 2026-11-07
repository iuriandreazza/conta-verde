/// A name typed by a person, made safe for one line of a subject or a log: a line break in it would add headers to a
/// message or forge lines in a log.
pub(crate) fn one_line(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}
