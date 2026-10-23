use std::io::{self, Cursor, Read};

pub(super) struct ReadFailure(Cursor<Vec<u8>>);

impl ReadFailure {
    pub(super) fn after(bytes: &[u8]) -> Self {
        Self(Cursor::new(bytes.to_vec()))
    }
}

impl Read for ReadFailure {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if self.0.position() == self.0.get_ref().len() as u64 {
            return Err(io::Error::other("injected stdout read failure"));
        }
        self.0.read(output)
    }
}
