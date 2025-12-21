/// Stub for potential future line wrap caching.
/// Currently the editor does inline wrapping during render.
#[derive(Debug, Clone, Default)]
pub struct WrapCache;

impl WrapCache {
    pub fn new() -> Self {
        Self
    }

    pub fn invalidate_line(&mut self, _row: usize) {}
    pub fn invalidate_from(&mut self, _row: usize) {}
    pub fn insert_line(&mut self, _row: usize) {}
    pub fn remove_line(&mut self, _row: usize) {}
}
