use std::sync::Arc;

use parking_lot::Mutex;
use tracing_subscriber::fmt::writer::MakeWriter;

enum Backend {
    Memory(Vec<u8>),
    File(),
}

#[derive(Clone)]
pub struct LogHandler {
    backend: Arc<Mutex<Backend>>,
}

impl Default for LogHandler {
    fn default() -> Self {
        Self {
            backend: Arc::new(Mutex::new(Backend::Memory(Vec::new()))),
        }
    }
}

impl std::io::Write for LogHandler {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match &mut *self.backend.lock() {
            Backend::Memory(out_buf) => out_buf.write(buf),
            Backend::File(out)
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match &mut *self.backend.lock() {
            Backend::Memory(_) => Ok(()),
        }
    }
}

impl<'a> MakeWriter<'a> for LogHandler {
    type Writer = LogHandler;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}
