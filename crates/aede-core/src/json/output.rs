//! Share the exact JSON renderer while bounding selected persistence outputs.

use std::fmt;

pub(super) trait Output: fmt::Write {
    fn push(&mut self, character: char);
    fn push_str(&mut self, text: &str);
    fn exhausted(&self) -> bool;
}

impl Output for String {
    fn push(&mut self, character: char) {
        String::push(self, character);
    }
    fn push_str(&mut self, text: &str) {
        String::push_str(self, text);
    }
    fn exhausted(&self) -> bool {
        false
    }
}

pub(super) struct Limited {
    text: String,
    maximum: usize,
    exhausted: bool,
}

impl Limited {
    pub(super) fn new(maximum: usize) -> Self {
        Self {
            text: String::new(),
            maximum,
            exhausted: false,
        }
    }

    pub(super) fn finish(self) -> Result<String, fmt::Error> {
        if self.exhausted {
            Err(fmt::Error)
        } else {
            Ok(self.text)
        }
    }

    fn append(&mut self, text: &str) {
        if self.exhausted || text.len() > self.maximum.saturating_sub(self.text.len()) {
            self.exhausted = true;
        } else {
            self.text.push_str(text);
        }
    }
}

impl Output for Limited {
    fn push(&mut self, character: char) {
        let mut bytes = [0; 4];
        self.append(character.encode_utf8(&mut bytes));
    }
    fn push_str(&mut self, text: &str) {
        self.append(text);
    }
    fn exhausted(&self) -> bool {
        self.exhausted
    }
}

impl fmt::Write for Limited {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        self.append(text);
        // Number/escape formatting errors remain retained in the sink, even
        // though the original infallible renderer does not return each write.
        if self.exhausted {
            Err(fmt::Error)
        } else {
            Ok(())
        }
    }
}

/// Check the same representation without allocating a second serialized store.
pub(super) struct Counter {
    length: usize,
    maximum: usize,
    exhausted: bool,
}

impl Counter {
    pub(super) fn new(maximum: usize) -> Self {
        Self {
            length: 0,
            maximum,
            exhausted: false,
        }
    }

    pub(super) fn finish(self) -> fmt::Result {
        if self.exhausted {
            Err(fmt::Error)
        } else {
            Ok(())
        }
    }

    fn append(&mut self, length: usize) {
        if self.exhausted || length > self.maximum.saturating_sub(self.length) {
            self.exhausted = true;
        } else {
            self.length += length;
        }
    }
}

impl Output for Counter {
    fn push(&mut self, character: char) {
        self.append(character.len_utf8());
    }
    fn push_str(&mut self, text: &str) {
        self.append(text.len());
    }
    fn exhausted(&self) -> bool {
        self.exhausted
    }
}

impl fmt::Write for Counter {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        self.append(text.len());
        if self.exhausted {
            Err(fmt::Error)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
#[path = "output_tests.rs"]
mod tests;
