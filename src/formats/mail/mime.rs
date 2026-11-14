//! A MIME message (.eml, one message of an .mbox) as a Mail row plus its
//! attachments. mail-parser decodes headers, charsets and transfer
//! encodings; `body_text` turns an HTML-only body into text.

use mail_parser::{Address, MessageParser, MimeHeaders};

use super::{Attachment, Mail, MailBook, join};

fn addresses(a: Option<&Address>) -> String {
    join(
        a.into_iter()
            .flat_map(|a| a.iter())
            .map(|x| match (x.name(), x.address()) {
                (Some(n), Some(e)) => format!("{n} <{e}>"),
                (None, Some(e)) => e.to_string(),
                (Some(n), None) => n.to_string(),
                (None, None) => String::new(),
            }),
    )
}

/// Parse `bytes` and add the message to `book`. `with_bytes` false lists
/// attachments without their contents (enough for the table picker's counts).
pub fn push_message(book: &mut MailBook, bytes: &[u8], folder: &str, with_bytes: bool) {
    let Some(msg) = MessageParser::default().parse(bytes) else {
        return;
    };
    // The sender's time as written in the header, without its zone.
    let sent = msg.date().map(|d| {
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            d.year, d.month, d.day, d.hour, d.minute, d.second
        )
    });
    let mail = Mail {
        folder: folder.to_string(),
        sent,
        received: None,
        from: addresses(msg.from()),
        to: addresses(msg.to()),
        cc: addresses(msg.cc()),
        subject: msg.subject().unwrap_or_default().to_string(),
        body: msg.body_text(0).map(|b| b.into_owned()).unwrap_or_default(),
        message_id: msg.message_id().unwrap_or_default().to_string(),
    };
    let atts = msg
        .attachments()
        .map(|p| {
            // An email attached inside the mail is listed by name only.
            let inner = p.message();
            let file_name = p
                .attachment_name()
                .map(str::to_string)
                .or_else(|| inner.and_then(|m| m.subject()).map(|s| format!("{s}.eml")))
                .unwrap_or_default();
            Attachment {
                mail_id: 0,
                file_name,
                size: p.contents().len() as i64,
                mime: p
                    .content_type()
                    .map(|c| match c.subtype() {
                        Some(s) => format!("{}/{s}", c.ctype()),
                        None => c.ctype().to_string(),
                    })
                    .unwrap_or_default(),
                content: (with_bytes && inner.is_none()).then(|| p.contents().to_vec()),
            }
        })
        .collect();
    book.push_mail(mail, atts);
}

#[cfg(test)]
#[path = "mime_tests.rs"]
mod tests;
