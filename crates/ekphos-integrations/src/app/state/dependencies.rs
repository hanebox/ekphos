use super::*;

/// Time source used by transient UI state.
pub trait Clock: Send + Sync {
    fn now(&self) -> std::time::Instant;
    fn today(&self) -> chrono::NaiveDate;
}

#[derive(Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> std::time::Instant {
        std::time::Instant::now()
    }
    fn today(&self) -> chrono::NaiveDate {
        chrono::Local::now().date_naive()
    }
}

pub use crate::image_service::{DisabledNetworkImageService, NetworkImageService, SystemNetworkImageService};

#[derive(Clone)]
pub struct AppDependencies {
    pub config_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub clipboard: Arc<dyn crate::clipboard::Clipboard>,
    pub clock: Arc<dyn Clock>,
    pub network_images: Arc<dyn NetworkImageService>,
}

impl AppDependencies {
    pub fn production() -> Self {
        Self { config_dir: Config::config_dir(), cache_dir: default_cache_dir(), clipboard: crate::clipboard::default_clipboard(), clock: Arc::new(SystemClock), network_images: Arc::new(SystemNetworkImageService) }
    }

    pub fn headless(config_dir: PathBuf, cache_dir: PathBuf) -> Self {
        Self { config_dir, cache_dir, clipboard: Arc::new(crate::clipboard::MemoryClipboard::default()), clock: Arc::new(SystemClock), network_images: Arc::new(DisabledNetworkImageService) }
    }
}
