//! Bounded XML subset used for device descriptions and SOAP, without DTDs.

use std::collections::BTreeMap;

const MAX_DEPTH: usize = 32;
const MAX_NODES: usize = 8192;
const MAX_BYTES: usize = 512 * 1024;

#[derive(Debug)]
pub(super) struct Node {
    pub name: String,
    pub namespace: String,
    pub text: String,
    pub children: Vec<Node>,
}

impl Node {
    pub fn child(&self, name: &str) -> Result<&Self, String> {
        self.optional(name)?
            .ok_or_else(|| format!("missing XML field {name}"))
    }

    pub fn optional(&self, name: &str) -> Result<Option<&Self>, String> {
        let mut found = self.children.iter().filter(|node| node.name == name);
        let result = found.next();
        if found.next().is_some() {
            return Err(format!("duplicate XML field {name}"));
        }
        Ok(result)
    }

    pub fn value(&self, name: &str) -> Result<&str, String> {
        let node = self.child(name)?;
        if !node.children.is_empty() {
            return Err(format!("nested XML field {name}"));
        }
        Ok(node.text.trim())
    }
}

pub(super) fn escape(value: &str) -> Result<String, String> {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if !xml_character(character) {
            return Err("invalid character in XML value".into());
        }
        escaped.push_str(match character {
            '&' => "&amp;",
            '<' => "&lt;",
            '>' => "&gt;",
            '"' => "&quot;",
            '\'' => "&apos;",
            _ => {
                escaped.push(character);
                continue;
            }
        });
    }
    Ok(escaped)
}

fn xml_character(value: char) -> bool {
    matches!(value, '\t' | '\n' | '\r' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')
}

fn decode(value: &str) -> Result<String, String> {
    let mut result = String::with_capacity(value.len());
    let mut remainder = value;
    while let Some(offset) = remainder.find('&') {
        result.push_str(&remainder[..offset]);
        remainder = &remainder[offset + 1..];
        let end = remainder.find(';').ok_or("unterminated XML entity")?;
        let entity = &remainder[..end];
        let decoded = match entity {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            _ => {
                let number = if let Some(number) = entity.strip_prefix("#x") {
                    u32::from_str_radix(number, 16).ok()
                } else if let Some(number) = entity.strip_prefix('#') {
                    number.parse::<u32>().ok()
                } else {
                    None
                };
                number
                    .and_then(char::from_u32)
                    .ok_or("unsupported XML entity")?
            }
        };
        if !xml_character(decoded) {
            return Err("invalid XML character entity".into());
        }
        result.push(decoded);
        remainder = &remainder[end + 1..];
    }
    result.push_str(remainder);
    Ok(result)
}

pub(super) fn parse(value: &str) -> Result<Node, String> {
    if value.len() > MAX_BYTES || value.chars().any(|value| !xml_character(value)) {
        return Err("device XML exceeds its limit or contains invalid characters".into());
    }
    let mut parser = Parser {
        source: value.strip_prefix('\u{feff}').unwrap_or(value),
        position: 0,
        nodes: 0,
    };
    parser.whitespace();
    if parser.rest().starts_with("<?xml ") {
        let end = parser
            .rest()
            .find("?>")
            .ok_or("unterminated XML declaration")?;
        if parser.rest()[..end].contains("<!") {
            return Err("invalid XML declaration".into());
        }
        parser.position += end + 2;
    }
    parser.misc()?;
    let mut namespaces = BTreeMap::new();
    namespaces.insert(
        "xml".to_string(),
        "http://www.w3.org/XML/1998/namespace".to_string(),
    );
    let root = parser.element(0, &namespaces)?;
    parser.misc()?;
    if !parser.rest().is_empty() {
        return Err("trailing device XML".into());
    }
    Ok(root)
}

struct Parser<'a> {
    source: &'a str,
    position: usize,
    nodes: usize,
}

impl Parser<'_> {
    fn rest(&self) -> &str {
        &self.source[self.position..]
    }

    fn whitespace(&mut self) {
        self.position += self
            .rest()
            .bytes()
            .take_while(|b| matches!(b, b' ' | b'\n' | b'\r' | b'\t'))
            .count();
    }

    fn misc(&mut self) -> Result<(), String> {
        loop {
            self.whitespace();
            if !self.rest().starts_with("<!--") {
                return Ok(());
            }
            self.comment()?;
        }
    }

    fn comment(&mut self) -> Result<(), String> {
        let end = self.rest().find("-->").ok_or("unterminated XML comment")?;
        if end < 4 || self.rest()[4..end].contains("--") {
            return Err("invalid XML comment".into());
        }
        self.position += end + 3;
        Ok(())
    }

    fn name(&mut self) -> Result<String, String> {
        let count = self
            .rest()
            .bytes()
            .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b':' | b'-' | b'.'))
            .count();
        if count > 128 {
            return Err("XML name exceeds its limit".into());
        }
        let value = self.rest()[..count].to_owned();
        if value.is_empty()
            || !value.as_bytes()[0].is_ascii_alphabetic() && value.as_bytes()[0] != b'_'
            || value.matches(':').count() > 1
        {
            return Err("unsupported XML name".into());
        }
        self.position += count;
        Ok(value)
    }

    fn element(
        &mut self,
        depth: usize,
        inherited: &BTreeMap<String, String>,
    ) -> Result<Node, String> {
        if depth >= MAX_DEPTH || self.nodes >= MAX_NODES {
            return Err("device XML nesting/node limit exceeded".into());
        }
        self.nodes += 1;
        if !self.rest().starts_with('<') {
            return Err("expected XML element".into());
        }
        self.position += 1;
        let qualified = self.name()?;
        let mut namespaces = inherited.clone();
        let mut attributes = BTreeMap::new();
        let self_closed;
        loop {
            let before = self.position;
            self.whitespace();
            if self.rest().starts_with("/>") {
                self.position += 2;
                self_closed = true;
                break;
            }
            if self.rest().starts_with('>') {
                self.position += 1;
                self_closed = false;
                break;
            }
            if before == self.position {
                return Err("missing XML attribute whitespace".into());
            }
            let key = self.name()?;
            if attributes.len() == 64 {
                return Err("too many XML attributes".into());
            }
            self.whitespace();
            if !self.rest().starts_with('=') {
                return Err("invalid XML attribute".into());
            }
            self.position += 1;
            self.whitespace();
            let quote = self
                .rest()
                .chars()
                .next()
                .ok_or("truncated XML attribute")?;
            if quote != '\'' && quote != '"' {
                return Err("unquoted XML attribute".into());
            }
            self.position += 1;
            let end = self.rest().find(quote).ok_or("truncated XML attribute")?;
            let raw = &self.rest()[..end];
            if raw.len() > 4096 || raw.contains('<') {
                return Err("invalid XML attribute value".into());
            }
            let value = decode(raw)?;
            self.position += end + 1;
            if attributes.insert(key.clone(), value.clone()).is_some() {
                return Err("duplicate XML attribute".into());
            }
            if key == "xmlns" {
                if value.len() > 256 {
                    return Err("XML namespace exceeds its limit".into());
                }
                namespaces.insert(String::new(), value);
            } else if let Some(prefix) = key.strip_prefix("xmlns:") {
                if value.len() > 256 {
                    return Err("XML namespace exceeds its limit".into());
                }
                if prefix == "xmlns"
                    || prefix == "xml" && value != "http://www.w3.org/XML/1998/namespace"
                {
                    return Err("reserved XML namespace declaration".into());
                }
                namespaces.insert(prefix.to_owned(), value);
            }
            if namespaces.len() > 32 {
                return Err("too many XML namespaces".into());
            }
        }
        let (prefix, name) = qualified.split_once(':').unwrap_or(("", &qualified));
        if name.is_empty()
            || !name.as_bytes()[0].is_ascii_alphabetic() && name.as_bytes()[0] != b'_'
        {
            return Err("invalid XML qualified name".into());
        }
        let namespace = namespaces.get(prefix).cloned().unwrap_or_default();
        if !prefix.is_empty() && namespace.is_empty() {
            return Err("undeclared XML namespace".into());
        }
        for key in attributes.keys() {
            if let Some((prefix, _)) = key.split_once(':')
                && prefix != "xmlns"
                && !namespaces.contains_key(prefix)
            {
                return Err("undeclared XML attribute namespace".into());
            }
        }
        let mut node = Node {
            name: name.into(),
            namespace,
            text: String::new(),
            children: Vec::new(),
        };
        if self_closed {
            return Ok(node);
        }
        loop {
            if self.rest().starts_with("</") {
                self.position += 2;
                let close = self.name()?;
                self.whitespace();
                if close != qualified || !self.rest().starts_with('>') {
                    return Err("mismatched XML closing element".into());
                }
                self.position += 1;
                return Ok(node);
            }
            if self.rest().starts_with("<!--") {
                self.comment()?;
            } else if self.rest().starts_with("<![CDATA[") {
                self.position += 9;
                let end = self.rest().find("]]>").ok_or("unterminated XML CDATA")?;
                node.text.push_str(&self.rest()[..end]);
                self.position += end + 3;
            } else if self.rest().starts_with("<!") || self.rest().starts_with("<?") {
                return Err("DTDs, entities and processing instructions are forbidden".into());
            } else if self.rest().starts_with('<') {
                node.children.push(self.element(depth + 1, &namespaces)?);
            } else {
                let end = self.rest().find('<').ok_or("truncated XML content")?;
                let raw = &self.rest()[..end];
                if raw.contains("]]>") {
                    return Err("invalid XML text".into());
                }
                node.text.push_str(&decode(raw)?);
                self.position += end;
            }
        }
    }
}

#[cfg(test)]
#[path = "upnp_xml_tests.rs"]
mod tests;
