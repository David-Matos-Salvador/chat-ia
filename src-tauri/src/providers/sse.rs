//! Lector de Server-Sent Events (SSE).
//!
//! Las APIs envían la respuesta en trozos de red que no respetan los límites de
//! línea ni de carácter. Este lector acumula bytes, corta por líneas completas y
//! entrega cada evento cuando llega la línea en blanco que lo cierra.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    /// Valor de `event:` (Anthropic lo usa; OpenAI no).
    pub event: Option<String>,
    /// Líneas `data:` unidas con saltos de línea.
    pub data: String,
}

#[derive(Default)]
pub struct SseParser {
    buffer: Vec<u8>,
    event: Option<String>,
    data: Vec<String>,
}

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Añade un trozo y devuelve los eventos que quedaron completos.
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<SseEvent> {
        self.buffer.extend_from_slice(chunk);
        let mut events = Vec::new();
        while let Some(pos) = self.buffer.iter().position(|&b| b == b'\n') {
            let line_bytes: Vec<u8> = self.buffer.drain(..=pos).collect();
            // La línea está completa, así que no corta ningún carácter UTF-8.
            let line = String::from_utf8_lossy(&line_bytes);
            let line = line.trim_end_matches(['\n', '\r']);
            if let Some(event) = self.handle_line(line) {
                events.push(event);
            }
        }
        events
    }

    /// Entrega el último evento si el servidor cerró sin línea en blanco final.
    pub fn finish(&mut self) -> Option<SseEvent> {
        if !self.buffer.is_empty() {
            let rest = String::from_utf8_lossy(&std::mem::take(&mut self.buffer)).to_string();
            if let Some(event) = self.handle_line(rest.trim_end_matches('\r')) {
                return Some(event);
            }
        }
        self.dispatch()
    }

    fn handle_line(&mut self, line: &str) -> Option<SseEvent> {
        if line.is_empty() {
            return self.dispatch();
        }
        if line.starts_with(':') {
            return None; // comentario (keep-alive)
        }
        let (field, value) = match line.split_once(':') {
            Some((f, v)) => (f, v.strip_prefix(' ').unwrap_or(v)),
            None => (line, ""),
        };
        match field {
            "event" => self.event = Some(value.to_string()),
            "data" => self.data.push(value.to_string()),
            _ => {} // id, retry: no se usan
        }
        None
    }

    fn dispatch(&mut self) -> Option<SseEvent> {
        if self.data.is_empty() {
            self.event = None;
            return None;
        }
        Some(SseEvent {
            event: self.event.take(),
            data: std::mem::take(&mut self.data).join("\n"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_all(chunks: &[&[u8]]) -> Vec<SseEvent> {
        let mut p = SseParser::new();
        let mut out: Vec<SseEvent> = chunks.iter().flat_map(|c| p.feed(c)).collect();
        out.extend(p.finish());
        out
    }

    #[test]
    fn parses_named_events() {
        let events = feed_all(&[b"event: message_start\ndata: {\"a\":1}\n\nevent: ping\ndata: {}\n\n"]);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event.as_deref(), Some("message_start"));
        assert_eq!(events[0].data, "{\"a\":1}");
        assert_eq!(events[1].event.as_deref(), Some("ping"));
    }

    #[test]
    fn handles_chunks_split_anywhere_including_inside_utf8() {
        let full = "data: {\"t\":\"canción ñandú 🚀\"}\n\ndata: [DONE]\n\n".as_bytes();
        // Partir en trozos de 1 byte cubre todos los cortes posibles.
        let chunks: Vec<&[u8]> = full.chunks(1).collect();
        let events = feed_all(&chunks);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].data, "{\"t\":\"canción ñandú 🚀\"}");
        assert_eq!(events[1].data, "[DONE]");
    }

    #[test]
    fn accepts_crlf_comments_and_multiline_data() {
        let events = feed_all(&[b": keep-alive\r\ndata: linea 1\r\ndata: linea 2\r\n\r\n"]);
        assert_eq!(events, vec![SseEvent { event: None, data: "linea 1\nlinea 2".into() }]);
    }

    #[test]
    fn flushes_last_event_without_trailing_blank_line() {
        let events = feed_all(&[b"data: final"]);
        assert_eq!(events, vec![SseEvent { event: None, data: "final".into() }]);
    }

    #[test]
    fn ignores_events_without_data() {
        assert!(feed_all(&[b"event: ping\n\n"]).is_empty());
    }
}
