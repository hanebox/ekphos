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

/// Network boundary for remote image loading. Tests use the disabled service.
pub trait NetworkImageService: Send + Sync {
    fn fetch(&self, url: &str) -> Option<DynamicImage>;
}

#[derive(Debug, Default)]
pub struct SystemNetworkImageService;

impl NetworkImageService for SystemNetworkImageService {
    fn fetch(&self, url: &str) -> Option<DynamicImage> {
        fetch_remote_image_blocking(url)
    }
}

#[derive(Debug, Default)]
pub struct DisabledNetworkImageService;

impl NetworkImageService for DisabledNetworkImageService {
    fn fetch(&self, _url: &str) -> Option<DynamicImage> {
        None
    }
}

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
        Self {
            config_dir: Config::config_dir(),
            cache_dir: default_cache_dir(),
            clipboard: crate::clipboard::default_clipboard(),
            clock: Arc::new(SystemClock),
            network_images: Arc::new(SystemNetworkImageService),
        }
    }

    pub fn headless(config_dir: PathBuf, cache_dir: PathBuf) -> Self {
        Self {
            config_dir,
            cache_dir,
            clipboard: Arc::new(crate::clipboard::MemoryClipboard::default()),
            clock: Arc::new(SystemClock),
            network_images: Arc::new(DisabledNetworkImageService),
        }
    }
}
