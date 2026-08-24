use crate::app::NetworkImageService;
use image::{DynamicImage, ImageFormat, ImageReader, Limits};
use std::collections::{HashMap, VecDeque};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

pub const DEFAULT_IMAGE_MEMORY_BUDGET: usize = 16 * 1024 * 1024;
pub const MAX_IMAGE_DOWNLOAD_BYTES: usize = 10 * 1024 * 1024;
pub const MAX_IMAGE_DIMENSION: u32 = 8_192;
pub const MAX_DECODED_IMAGE_BYTES: usize = 64 * 1024 * 1024;
const CACHE_MAX_DIMENSION: u32 = 300;
const IMAGE_WORKERS: usize = 2;
const IMAGE_QUEUE_CAPACITY: usize = 32;
const MAX_PENDING_IMAGE_REQUESTS: usize = 16;
const WORKER_IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(250);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImageServiceStats {
    pub decoded_bytes: usize,
    pub decoded_entries: usize,
    pub pending_requests: usize,
    pub failed_requests: usize,
    pub live_workers: usize,
}

#[derive(Clone)]
enum ImageSource {
    Local(PathBuf),
    Remote(String),
}

struct ImageRequest {
    key: String,
    source: ImageSource,
    cache_path: PathBuf,
    generation: u64,
}

enum WorkerMessage {
    Load(ImageRequest),
    Shutdown,
}

struct ImageResult {
    key: String,
    generation: u64,
    result: Result<DynamicImage, String>,
}

struct DecodedEntry {
    image: Arc<DynamicImage>,
    bytes: usize,
}

pub struct ImageService {
    cache_dir: PathBuf,
    request_sender: SyncSender<WorkerMessage>,
    request_receiver: Arc<Mutex<Receiver<WorkerMessage>>>,
    result_sender: SyncSender<ImageResult>,
    result_receiver: Receiver<ImageResult>,
    network: Arc<dyn NetworkImageService>,
    worker_limit: usize,
    workers: Vec<JoinHandle<()>>,
    shutdown: Arc<AtomicBool>,
    generation: Arc<AtomicU64>,
    pending: HashMap<String, u64>,
    failures: HashMap<String, String>,
    decoded: HashMap<String, DecodedEntry>,
    lru: VecDeque<String>,
    decoded_bytes: usize,
    budget: usize,
}

impl ImageService {
    pub fn new(cache_dir: PathBuf, network: Arc<dyn NetworkImageService>) -> Self {
        Self::with_budget_and_workers(cache_dir, network, DEFAULT_IMAGE_MEMORY_BUDGET, IMAGE_WORKERS)
    }

    fn with_budget_and_workers(cache_dir: PathBuf, network: Arc<dyn NetworkImageService>, budget: usize, worker_count: usize) -> Self {
        let _ = std::fs::create_dir_all(&cache_dir);
        let (request_sender, request_receiver) = mpsc::sync_channel(IMAGE_QUEUE_CAPACITY);
        let (result_sender, result_receiver) = mpsc::sync_channel(IMAGE_QUEUE_CAPACITY);
        let request_receiver = Arc::new(Mutex::new(request_receiver));
        let shutdown = Arc::new(AtomicBool::new(false));
        let generation = Arc::new(AtomicU64::new(0));

        Self {
            cache_dir,
            request_sender,
            request_receiver,
            result_sender,
            result_receiver,
            network,
            worker_limit: worker_count,
            workers: Vec::with_capacity(worker_count),
            shutdown,
            generation,
            pending: HashMap::new(),
            failures: HashMap::new(),
            decoded: HashMap::new(),
            lru: VecDeque::new(),
            decoded_bytes: 0,
            budget,
        }
    }

    pub fn begin_document(&mut self, generation: u64) {
        self.generation.store(generation, Ordering::Release);
        self.pending.clear();
        self.failures.clear();
        self.decoded.clear();
        self.lru.clear();
        self.decoded_bytes = 0;
        self.drain_stale_results();
    }

    pub fn request_local(&mut self, key: &str, path: PathBuf) -> bool {
        self.request(key, ImageSource::Local(path))
    }

    pub fn request_remote(&mut self, key: &str, url: &str) -> bool {
        self.request(key, ImageSource::Remote(url.to_string()))
    }

    fn request(&mut self, key: &str, source: ImageSource) -> bool {
        if self.decoded.contains_key(key)
            || self.pending.contains_key(key)
            || self.failures.contains_key(key)
            || self.pending.len() >= MAX_PENDING_IMAGE_REQUESTS
        {
            return false;
        }
        let generation = self.generation.load(Ordering::Acquire);
        let request = ImageRequest {
            key: key.to_string(),
            source,
            cache_path: self.cache_path(key),
            generation,
        };
        match self.request_sender.try_send(WorkerMessage::Load(request)) {
            Ok(()) => {
                self.pending.insert(key.to_string(), generation);
                self.ensure_workers();
                if self.workers.is_empty() {
                    self.pending.remove(key);
                    self.failures.insert(key.to_string(), "image worker pool unavailable".to_string());
                    false
                } else {
                    true
                }
            }
            Err(TrySendError::Full(_)) => false,
            Err(TrySendError::Disconnected(_)) => {
                self.failures.insert(key.to_string(), "image worker pool disconnected".to_string());
                false
            }
        }
    }

    pub fn poll(&mut self) -> bool {
        if !self.pending.is_empty() {
            self.ensure_workers();
        }
        let mut changed = false;
        while let Ok(result) = self.result_receiver.try_recv() {
            let current_generation = self.generation.load(Ordering::Acquire);
            if result.generation != current_generation || self.pending.get(&result.key) != Some(&result.generation) {
                continue;
            }
            self.pending.remove(&result.key);
            match result.result {
                Ok(image) => self.insert_decoded(result.key, image),
                Err(error) => {
                    self.failures.insert(result.key, error);
                }
            }
            changed = true;
        }
        changed
    }

    pub fn decoded(&mut self, key: &str) -> Option<Arc<DynamicImage>> {
        let image = Arc::clone(&self.decoded.get(key)?.image);
        self.touch(key);
        Some(image)
    }

    pub fn insert_ready(&mut self, key: &str, image: DynamicImage) -> Result<(), String> {
        validate_image(&image)?;
        let image = resize_for_cache(image);
        write_cached_image(&self.cache_path(key), &image)?;
        self.insert_decoded(key.to_string(), image);
        Ok(())
    }

    pub fn load_cached_now(&mut self, key: &str) -> Option<DynamicImage> {
        if let Some(image) = self.decoded(key) {
            return Some((*image).clone());
        }
        let image = decode_path(&self.cache_path(key)).ok()?;
        self.insert_decoded(key.to_string(), image.clone());
        Some(image)
    }

    pub fn is_pending(&self, key: &str) -> bool {
        self.pending.contains_key(key)
    }

    pub fn is_failed(&self, key: &str) -> bool {
        self.failures.contains_key(key)
    }

    pub fn is_cached_on_disk(&self, key: &str) -> bool {
        self.cache_path(key).is_file()
    }

    pub fn decoded_bytes(&self) -> usize {
        self.decoded_bytes
    }

    pub fn trim_to_budget(&mut self, budget: usize) {
        while self.decoded_bytes > budget && self.decoded.len() > 1 {
            let Some(oldest) = self.lru.pop_front() else {
                break;
            };
            if let Some(entry) = self.decoded.remove(&oldest) {
                self.decoded_bytes = self.decoded_bytes.saturating_sub(entry.bytes);
            }
        }
    }

    pub fn stats(&self) -> ImageServiceStats {
        ImageServiceStats {
            decoded_bytes: self.decoded_bytes,
            decoded_entries: self.decoded.len(),
            pending_requests: self.pending.len(),
            failed_requests: self.failures.len(),
            live_workers: self.workers.iter().filter(|worker| !worker.is_finished()).count(),
        }
    }

    fn cache_path(&self, key: &str) -> PathBuf {
        self.cache_dir.join(cache_key_to_filename(key))
    }

    fn ensure_workers(&mut self) {
        let mut active_workers = Vec::with_capacity(self.worker_limit);
        for worker in self.workers.drain(..) {
            if worker.is_finished() {
                let _ = worker.join();
            } else {
                active_workers.push(worker);
            }
        }
        self.workers = active_workers;
        if !self.workers.is_empty() || self.worker_limit == 0 {
            return;
        }
        for index in 0..self.worker_limit {
            let receiver = Arc::clone(&self.request_receiver);
            let sender = self.result_sender.clone();
            let network = Arc::clone(&self.network);
            let shutdown_signal = Arc::clone(&self.shutdown);
            let generation_signal = Arc::clone(&self.generation);
            if let Ok(worker) = std::thread::Builder::new().name(format!("image-worker-{index}")).spawn(move || {
                image_worker_loop(receiver, sender, network, shutdown_signal, generation_signal);
            }) {
                self.workers.push(worker);
            }
        }
    }

    fn insert_decoded(&mut self, key: String, image: DynamicImage) {
        if let Some(previous) = self.decoded.remove(&key) {
            self.decoded_bytes = self.decoded_bytes.saturating_sub(previous.bytes);
            self.lru.retain(|candidate| candidate != &key);
        }
        let bytes = decoded_image_bytes(&image);
        self.decoded_bytes = self.decoded_bytes.saturating_add(bytes);
        self.lru.push_back(key.clone());
        self.decoded.insert(key, DecodedEntry { image: Arc::new(image), bytes });
        while self.decoded_bytes > self.budget && self.decoded.len() > 1 {
            let Some(oldest) = self.lru.pop_front() else {
                break;
            };
            if let Some(entry) = self.decoded.remove(&oldest) {
                self.decoded_bytes = self.decoded_bytes.saturating_sub(entry.bytes);
            }
        }
    }

    fn touch(&mut self, key: &str) {
        self.lru.retain(|candidate| candidate != key);
        self.lru.push_back(key.to_string());
    }

    fn drain_stale_results(&mut self) {
        while self.result_receiver.try_recv().is_ok() {}
    }
}

impl Drop for ImageService {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        for _ in 0..self.workers.len() {
            let _ = self.request_sender.send(WorkerMessage::Shutdown);
        }
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

fn image_worker_loop(
    receiver: Arc<Mutex<Receiver<WorkerMessage>>>,
    sender: SyncSender<ImageResult>,
    network: Arc<dyn NetworkImageService>,
    shutdown: Arc<AtomicBool>,
    generation: Arc<AtomicU64>,
) {
    loop {
        let message = match receiver.lock() {
            Ok(receiver) => receiver.recv_timeout(WORKER_IDLE_TIMEOUT),
            Err(_) => return,
        };
        let message = match message {
            Ok(message) => message,
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => return,
        };
        let WorkerMessage::Load(request) = message else {
            return;
        };
        if shutdown.load(Ordering::Acquire) {
            return;
        }
        if request.generation != generation.load(Ordering::Acquire) {
            continue;
        }

        let result = load_request(&request, network.as_ref());
        if request.generation != generation.load(Ordering::Acquire) {
            continue;
        }
        if sender
            .send(ImageResult {
                key: request.key,
                generation: request.generation,
                result,
            })
            .is_err()
        {
            return;
        }
    }
}

fn load_request(request: &ImageRequest, network: &dyn NetworkImageService) -> Result<DynamicImage, String> {
    let image = match &request.source {
        ImageSource::Local(path) => decode_path(path)?,
        ImageSource::Remote(url) => {
            if request.cache_path.is_file() {
                match decode_path(&request.cache_path) {
                    Ok(image) => return Ok(image),
                    Err(_) => {
                        let _ = std::fs::remove_file(&request.cache_path);
                    }
                }
            }
            let image = network.fetch(url).ok_or_else(|| "remote image fetch failed".to_string())?;
            validate_image(&image)?;
            let image = resize_for_cache(image);
            write_cached_image(&request.cache_path, &image)?;
            image
        }
    };
    validate_image(&image)?;
    Ok(resize_for_cache(image))
}

pub(crate) fn decode_memory(bytes: &[u8]) -> Result<DynamicImage, String> {
    if bytes.len() > MAX_IMAGE_DOWNLOAD_BYTES {
        return Err("encoded image exceeds download limit".to_string());
    }
    let mut reader = ImageReader::new(Cursor::new(bytes));
    reader = reader.with_guessed_format().map_err(|error| error.to_string())?;
    reader.limits(image_limits());
    let image = reader.decode().map_err(|error| error.to_string())?;
    validate_image(&image)?;
    Ok(image)
}

fn decode_path(path: &Path) -> Result<DynamicImage, String> {
    let metadata = std::fs::metadata(path).map_err(|error| error.to_string())?;
    if metadata.len() > MAX_IMAGE_DOWNLOAD_BYTES as u64 {
        return Err("encoded image exceeds byte limit".to_string());
    }
    let mut reader = ImageReader::open(path)
        .map_err(|error| error.to_string())?
        .with_guessed_format()
        .map_err(|error| error.to_string())?;
    reader.limits(image_limits());
    let image = reader.decode().map_err(|error| error.to_string())?;
    validate_image(&image)?;
    Ok(image)
}

fn image_limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_DIMENSION);
    limits.max_image_height = Some(MAX_IMAGE_DIMENSION);
    limits.max_alloc = Some(MAX_DECODED_IMAGE_BYTES as u64);
    limits
}

fn validate_image(image: &DynamicImage) -> Result<(), String> {
    if image.width() == 0 || image.height() == 0 {
        return Err("image has zero dimensions".to_string());
    }
    if image.width() > MAX_IMAGE_DIMENSION || image.height() > MAX_IMAGE_DIMENSION {
        return Err("image dimensions exceed limit".to_string());
    }
    if decoded_image_bytes(image) > MAX_DECODED_IMAGE_BYTES {
        return Err("decoded image exceeds memory limit".to_string());
    }
    Ok(())
}

pub(crate) fn decoded_image_bytes(image: &DynamicImage) -> usize {
    image.as_bytes().len()
}

fn resize_for_cache(image: DynamicImage) -> DynamicImage {
    if image.width() <= CACHE_MAX_DIMENSION && image.height() <= CACHE_MAX_DIMENSION {
        return image;
    }
    image.resize(CACHE_MAX_DIMENSION, CACHE_MAX_DIMENSION, image::imageops::FilterType::Triangle)
}

fn write_cached_image(path: &Path, image: &DynamicImage) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let temporary = path.with_extension("png.tmp");
    image.save_with_format(&temporary, ImageFormat::Png).map_err(|error| error.to_string())?;
    std::fs::rename(&temporary, path).map_err(|error| error.to_string())
}

fn cache_key_to_filename(key: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    format!("{:x}.png", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    #[derive(Default)]
    struct FixtureNetwork {
        calls: AtomicUsize,
        image: Mutex<Option<DynamicImage>>,
    }

    impl NetworkImageService for FixtureNetwork {
        fn fetch(&self, _url: &str) -> Option<DynamicImage> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            self.image.lock().unwrap().clone()
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("ekphos-phase8-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn wait_for(service: &mut ImageService) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !service.poll() && Instant::now() < deadline {
            std::thread::yield_now();
        }
    }

    #[test]
    fn duplicate_remote_requests_share_one_worker_job_and_disk_entry() {
        let network = Arc::new(FixtureNetwork {
            calls: AtomicUsize::new(0),
            image: Mutex::new(Some(DynamicImage::ImageRgba8(RgbaImage::from_pixel(32, 32, Rgba([1, 2, 3, 255]))))),
        });
        let cache = temp_dir("dedup");
        let mut service = ImageService::new(cache.clone(), network.clone());
        service.begin_document(1);

        assert!(service.request_remote("same", "https://fixtures.invalid/same.png"));
        assert!(!service.request_remote("same", "https://fixtures.invalid/same.png"));
        wait_for(&mut service);

        assert!(service.decoded("same").is_some());
        assert!(service.is_cached_on_disk("same"));
        assert_eq!(network.calls.load(Ordering::Relaxed), 1);

        drop(service);
        let warm_network = Arc::new(FixtureNetwork::default());
        let mut warm = ImageService::new(cache, warm_network.clone());
        warm.begin_document(2);
        assert!(warm.request_remote("same", "https://fixtures.invalid/same.png"));
        wait_for(&mut warm);
        assert!(warm.decoded("same").is_some());
        assert_eq!(warm_network.calls.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn document_switch_cancels_and_releases_decoded_work() {
        let cache = temp_dir("cancel");
        let mut service = ImageService::new(cache, Arc::new(FixtureNetwork::default()));
        service.begin_document(1);
        service
            .insert_ready("old", DynamicImage::ImageRgba8(RgbaImage::from_pixel(16, 16, Rgba([0, 0, 0, 255]))))
            .unwrap();
        assert!(service.decoded_bytes() > 0);

        service.begin_document(2);
        assert_eq!(service.decoded_bytes(), 0);
        assert_eq!(service.stats().pending_requests, 0);
    }

    #[test]
    fn corrupt_and_oversized_images_fail_without_retention() {
        let cache = temp_dir("limits");
        let corrupt = cache.join("corrupt.png");
        std::fs::write(&corrupt, b"not an image").unwrap();
        let mut service = ImageService::new(cache, Arc::new(FixtureNetwork::default()));
        service.begin_document(1);
        assert!(service.request_local("corrupt", corrupt));
        wait_for(&mut service);
        assert!(service.is_failed("corrupt"));
        assert_eq!(service.decoded_bytes(), 0);

        let oversized = DynamicImage::ImageRgba8(RgbaImage::new(MAX_IMAGE_DIMENSION + 1, 1));
        assert!(service.insert_ready("oversized", oversized).is_err());
    }

    #[test]
    fn decoded_cache_is_byte_weighted() {
        let cache = temp_dir("lru");
        let mut service = ImageService::with_budget_and_workers(cache, Arc::new(FixtureNetwork::default()), 4, 0);
        service
            .insert_ready("one", DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 2, Rgba([1, 1, 1, 255]))))
            .unwrap();
        service
            .insert_ready("two", DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 2, Rgba([2, 2, 2, 255]))))
            .unwrap();

        assert_eq!(service.stats().decoded_entries, 1);
        assert!(service.decoded_bytes() > 4);
    }

    #[test]
    fn pending_queue_is_capped_and_failed_remote_fetch_settles() {
        let cache = temp_dir("queue");
        let local = cache.join("tiny.png");
        RgbaImage::from_pixel(1, 1, Rgba([0, 0, 0, 255])).save(&local).unwrap();
        let mut service = ImageService::new(cache, Arc::new(FixtureNetwork::default()));
        service.begin_document(1);
        for index in 0..100 {
            service.request_local(&format!("image-{index}"), local.clone());
        }
        assert!(service.stats().pending_requests <= MAX_PENDING_IMAGE_REQUESTS);

        service.begin_document(2);
        assert!(service.request_remote("failed", "https://fixtures.invalid/failed.png"));
        wait_for(&mut service);
        assert!(service.is_failed("failed"));
        assert_eq!(service.stats().pending_requests, 0);
    }

    #[test]
    fn idle_worker_pool_exits_and_restarts_on_demand() {
        let cache = temp_dir("worker-idle");
        let local = cache.join("tiny.png");
        RgbaImage::from_pixel(2, 2, Rgba([4, 5, 6, 255])).save(&local).unwrap();
        let mut service = ImageService::new(cache, Arc::new(FixtureNetwork::default()));
        service.begin_document(1);
        assert!(service.request_local("first", local.clone()));
        wait_for(&mut service);
        assert_eq!(service.stats().live_workers, IMAGE_WORKERS);

        std::thread::sleep(WORKER_IDLE_TIMEOUT * (IMAGE_WORKERS as u32 + 1));
        assert_eq!(service.stats().live_workers, 0);
        assert!(service.request_local("second", local));
        wait_for(&mut service);
        assert!(service.decoded("second").is_some());
    }
}
