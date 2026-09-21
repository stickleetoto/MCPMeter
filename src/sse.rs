const DEFAULT_MAX_EVENT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug)]
pub struct SseParser {
    line_buffer: Vec<u8>,
    data_lines: Vec<Vec<u8>>,
    data_bytes: usize,
    overflowed: bool,
    max_event_bytes: usize,
}

impl Default for SseParser {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_EVENT_BYTES)
    }
}

impl SseParser {
    pub fn new(max_event_bytes: usize) -> Self {
        Self {
            line_buffer: Vec::new(),
            data_lines: Vec::new(),
            data_bytes: 0,
            overflowed: false,
            max_event_bytes,
        }
    }

    pub fn push(&mut self, chunk: &[u8]) -> Vec<Vec<u8>> {
        self.line_buffer.extend_from_slice(chunk);
        let mut events = Vec::new();

        while let Some(newline) = self.line_buffer.iter().position(|byte| *byte == b'\n') {
            let mut line: Vec<u8> = self.line_buffer.drain(..=newline).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            self.process_line(&line, &mut events);
        }

        events
    }

    pub fn finish(&mut self) -> Vec<Vec<u8>> {
        let mut events = Vec::new();

        if !self.line_buffer.is_empty() {
            let mut line = std::mem::take(&mut self.line_buffer);
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            self.process_line(&line, &mut events);
        }

        self.dispatch(&mut events);
        events
    }

    fn process_line(&mut self, line: &[u8], events: &mut Vec<Vec<u8>>) {
        if line.is_empty() {
            self.dispatch(events);
            return;
        }

        if line.starts_with(b":") {
            return;
        }

        let (field, mut value) = match line.iter().position(|byte| *byte == b':') {
            Some(colon) => (&line[..colon], &line[colon + 1..]),
            None => (line, &[][..]),
        };

        if field != b"data" {
            return;
        }

        if value.first() == Some(&b' ') {
            value = &value[1..];
        }

        let separator_bytes = usize::from(!self.data_lines.is_empty());
        let next_size = self
            .data_bytes
            .saturating_add(separator_bytes)
            .saturating_add(value.len());

        if next_size > self.max_event_bytes {
            self.overflowed = true;
            self.data_lines.clear();
            self.data_bytes = 0;
            return;
        }

        if !self.overflowed {
            self.data_lines.push(value.to_vec());
            self.data_bytes = next_size;
        }
    }

    fn dispatch(&mut self, events: &mut Vec<Vec<u8>>) {
        if self.overflowed {
            self.reset_event();
            return;
        }

        if self.data_lines.is_empty() {
            return;
        }

        let mut payload = Vec::with_capacity(self.data_bytes);
        for (index, line) in self.data_lines.iter().enumerate() {
            if index > 0 {
                payload.push(b'\n');
            }
            payload.extend_from_slice(line);
        }

        events.push(payload);
        self.reset_event();
    }

    fn reset_event(&mut self) {
        self.data_lines.clear();
        self.data_bytes = 0;
        self.overflowed = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconstructs_multiline_data_across_transport_chunks() {
        let mut parser = SseParser::default();
        let mut events = Vec::new();

        events.extend(parser.push(b": keepalive\r\nda"));
        events.extend(parser.push(b"ta: {\"jsonrpc\":\"2.0\",\r\n"));
        events.extend(parser.push(b"data: \"id\":7}\r\n\r"));
        events.extend(parser.push(b"\n"));

        assert_eq!(events, vec![b"{\"jsonrpc\":\"2.0\",\n\"id\":7}".to_vec()]);
    }

    #[test]
    fn ignores_non_data_fields_and_comments() {
        let mut parser = SseParser::default();
        let events = parser.push(
            b"event: message\nid: 4\nretry: 1000\n: heartbeat\ndata: {\"jsonrpc\":\"2.0\"}\n\n",
        );

        assert_eq!(events, vec![b"{\"jsonrpc\":\"2.0\"}".to_vec()]);
    }

    #[test]
    fn drops_oversized_event_without_poisoning_next_event() {
        let mut parser = SseParser::new(4);
        assert!(parser.push(b"data: 12345\n\n").is_empty());
        assert_eq!(parser.push(b"data: ok\n\n"), vec![b"ok".to_vec()]);
    }

    #[test]
    fn finish_dispatches_final_unterminated_event() {
        let mut parser = SseParser::default();
        assert!(parser.push(b"data: final").is_empty());
        assert_eq!(parser.finish(), vec![b"final".to_vec()]);
    }
}
