pub struct ReceiptBuilder {
    buffer: Vec<u8>,
}

impl ReceiptBuilder {
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    pub fn initialize(mut self) -> Self {
        // ESC @: Initialize printer
        self.buffer.extend_from_slice(&[0x1B, 0x40]);
        self
    }

    pub fn align_center(mut self) -> Self {
        // ESC a n: Align (0: Left, 1: Center, 2: Right)
        self.buffer.extend_from_slice(&[0x1B, 0x61, 0x01]);
        self
    }

    pub fn align_left(mut self) -> Self {
        self.buffer.extend_from_slice(&[0x1B, 0x61, 0x00]);
        self
    }

    pub fn text(mut self, text: &str) -> Self {
        self.buffer.extend_from_slice(text.as_bytes());
        self
    }

    pub fn text_line(mut self, text: &str) -> Self {
        self.buffer.extend_from_slice(text.as_bytes());
        self.buffer.push(0x0A); // LF
        self
    }

    pub fn empty_line(mut self) -> Self {
        self.buffer.push(0x0A);
        self
    }

    pub fn separator(self) -> Self {
        self.text_line("----------------------------------------")
    }

    pub fn kv(self, key: &str, value: &str) -> Self {
        // Format: "Key:           Value"
        // Adjust width as needed (e.g. 40 cols)
        let line = format!("{:<12}: {}", key, value);
        self.text_line(&line)
    }

    pub fn feed(mut self, n: u8) -> Self {
        // ESC d n: Print and feed n lines
        self.buffer.extend_from_slice(&[0x1B, 0x64, n]);
        self
    }

    pub fn cut(mut self) -> Self {
        // GS V m: Cut paper (Feeds then cuts)
        // Standard GS V 66 0 (Feed to cut pos and cut)
        self.buffer.extend_from_slice(&[0x1D, 0x56, 66, 0]);
        self
    }

    pub fn build(self) -> Vec<u8> {
        self.buffer
    }
}
