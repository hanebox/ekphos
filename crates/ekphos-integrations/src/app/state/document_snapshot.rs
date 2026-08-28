use std::ops::Range;
use std::sync::Arc;

/// A byte range into a [`DocumentSnapshot`].
///
/// Fixed-width offsets keep the normal-mode parse compact on large notes and
/// make accidental owned substrings unnecessary.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DocumentRange {
    start: u32,
    end: u32,
}

impl DocumentRange {
    pub fn new(start: usize, end: usize) -> Self {
        debug_assert!(start <= end);
        Self { start: u32::try_from(start).expect("active document exceeds 4 GiB"), end: u32::try_from(end).expect("active document exceeds 4 GiB") }
    }

    pub fn start(self) -> usize {
        self.start as usize
    }

    pub fn end(self) -> usize {
        self.end as usize
    }

    pub fn len(self) -> usize {
        self.end().saturating_sub(self.start())
    }

    pub fn is_empty(self) -> bool {
        self.start == self.end
    }
}

/// The sole immutable representation of the active normal-mode document.
#[derive(Debug)]
pub struct DocumentSnapshot {
    body: Arc<str>,
    line_offsets: Box<[u32]>,
}

impl DocumentSnapshot {
    pub fn new(body: Arc<str>) -> Self {
        assert!(u32::try_from(body.len()).is_ok(), "active document exceeds 4 GiB");
        let mut offsets = Vec::with_capacity(body.as_bytes().iter().filter(|&&byte| byte == b'\n').count() + 1);
        if !body.is_empty() {
            offsets.push(0);
            for (index, byte) in body.as_bytes().iter().enumerate() {
                if *byte == b'\n' && index + 1 < body.len() {
                    offsets.push(u32::try_from(index + 1).expect("active document exceeds 4 GiB"));
                }
            }
        }
        Self { body, line_offsets: offsets.into_boxed_slice() }
    }

    pub fn body(&self) -> &str {
        &self.body
    }

    pub fn body_arc(&self) -> Arc<str> {
        Arc::clone(&self.body)
    }

    pub fn into_body(self) -> Arc<str> {
        self.body
    }

    pub fn line_count(&self) -> usize {
        self.line_offsets.len()
    }

    pub fn line_range(&self, line: usize) -> Option<DocumentRange> {
        let start = *self.line_offsets.get(line)? as usize;
        let mut end = self.line_offsets.get(line + 1).map_or(self.body.len(), |next| *next as usize);
        if end > start && self.body.as_bytes()[end - 1] == b'\n' {
            end -= 1;
        }
        if end > start && self.body.as_bytes()[end - 1] == b'\r' {
            end -= 1;
        }
        Some(DocumentRange::new(start, end))
    }

    pub fn line(&self, line: usize) -> Option<&str> {
        self.line_range(line).map(|range| self.slice(range))
    }

    pub fn slice(&self, range: DocumentRange) -> &str {
        self.body.get(range.start()..range.end()).unwrap_or("")
    }

    pub fn range_within_line(&self, line: usize, range: Range<usize>) -> Option<DocumentRange> {
        let line_range = self.line_range(line)?;
        (range.start <= range.end && range.end <= line_range.len()).then(|| DocumentRange::new(line_range.start() + range.start, line_range.start() + range.end))
    }

    pub fn retained_bytes(&self) -> usize {
        self.body.len() + self.offset_bytes()
    }

    pub fn offset_bytes(&self) -> usize {
        self.line_offsets.len() * std::mem::size_of::<u32>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_offsets_match_str_lines_for_unicode_crlf_and_trailing_newline() {
        for source in ["", "one", "one\n", "one\r\ntwo\n", "e\u{301}\n日本語\n😀"] {
            let snapshot = DocumentSnapshot::new(Arc::from(source));
            let expected: Vec<&str> = source.lines().collect();
            let actual: Vec<&str> = (0..snapshot.line_count()).filter_map(|line| snapshot.line(line)).collect();
            assert_eq!(actual, expected, "{source:?}");
        }
    }

    #[test]
    fn range_within_line_is_byte_exact() {
        let snapshot = DocumentSnapshot::new(Arc::from("a😀z\n"));
        let range = snapshot.range_within_line(0, 1..5).unwrap();
        assert_eq!(snapshot.slice(range), "😀");
    }
}
