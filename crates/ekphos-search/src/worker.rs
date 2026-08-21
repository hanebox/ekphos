use crate::SearchIndex;
use ekphos_core::NoteId;
use std::cmp::Ordering as CmpOrdering;
use std::collections::BinaryHeap;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

pub const INDEXED_RESULT_LIMIT: usize = 15_000;
pub const FALLBACK_RESULT_LIMIT: usize = 500;
const MAX_PREFIX_TERMS_SCANNED: usize = 15_000;
const MAX_PREFIX_POSTINGS: usize = 15_000;

#[derive(Debug, Clone)]
pub struct ContentSearchSource {
    pub note_id: NoteId,
    pub title: Box<str>,
    pub absolute_path: PathBuf,
}

/// A compact, non-hydrated result. Titles and snippets remain in the catalog
/// and source file until the UI asks for its visible result window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct SearchHit {
    pub note_id: NoteId,
    /// Zero-based source line.
    pub line_number: u32,
    /// Character range in the complete source line.
    pub match_start: u32,
    pub match_end: u32,
    pub score: i32,
}

#[derive(Debug)]
pub struct SearchResponse {
    pub query_id: u64,
    pub generation: u64,
    pub hits: Vec<SearchHit>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RankedHit {
    hit: SearchHit,
    title_rank: usize,
    source_rank: usize,
}

impl Ord for RankedHit {
    fn cmp(&self, other: &Self) -> CmpOrdering {
        other
            .hit
            .score
            .cmp(&self.hit.score)
            .then_with(|| self.title_rank.cmp(&other.title_rank))
            .then_with(|| self.hit.line_number.cmp(&other.hit.line_number))
            .then_with(|| self.source_rank.cmp(&other.source_rank))
    }
}

impl PartialOrd for RankedHit {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.cmp(other))
    }
}

struct SearchRequest {
    ticket: u64,
    query_id: u64,
    generation: u64,
    query: String,
    sources: Arc<[ContentSearchSource]>,
    index: Option<Arc<SearchIndex>>,
}

#[derive(Default)]
struct RequestState {
    request: Option<SearchRequest>,
    stop: bool,
}

struct Shared {
    request: Mutex<RequestState>,
    request_ready: Condvar,
    result: Mutex<Option<SearchResponse>>,
    current_ticket: AtomicU64,
    pending: AtomicBool,
}

/// One managed worker backed by replaceable request and result slots. Typing a
/// new query drops the unprocessed old request and cancels an in-flight scan at
/// the next line/file boundary.
pub struct SearchWorker {
    shared: Arc<Shared>,
    handle: Option<JoinHandle<()>>,
}

impl Default for SearchWorker {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchWorker {
    pub fn new() -> Self {
        let shared = Arc::new(Shared {
            request: Mutex::new(RequestState::default()),
            request_ready: Condvar::new(),
            result: Mutex::new(None),
            current_ticket: AtomicU64::new(0),
            pending: AtomicBool::new(false),
        });
        let worker_shared = Arc::clone(&shared);
        let handle = thread::Builder::new()
            .name("ekphos-search".to_string())
            .spawn(move || run_worker(worker_shared))
            .expect("failed to start search worker");
        Self { shared, handle: Some(handle) }
    }

    pub fn submit(&self, query_id: u64, generation: u64, query: String, sources: Arc<[ContentSearchSource]>, index: Option<Arc<SearchIndex>>) {
        let ticket = self.shared.current_ticket.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
        self.shared.pending.store(true, Ordering::Release);
        if let Ok(mut result) = self.shared.result.lock() {
            *result = None;
        }
        if let Ok(mut state) = self.shared.request.lock() {
            state.request = Some(SearchRequest {
                ticket,
                query_id,
                generation,
                query,
                sources,
                index,
            });
            self.shared.request_ready.notify_one();
        }
    }

    pub fn cancel(&self) {
        self.shared.current_ticket.fetch_add(1, Ordering::AcqRel);
        self.shared.pending.store(false, Ordering::Release);
        if let Ok(mut state) = self.shared.request.lock() {
            state.request = None;
        }
        if let Ok(mut result) = self.shared.result.lock() {
            *result = None;
        }
    }

    pub fn try_take(&self) -> Option<SearchResponse> {
        self.shared.result.lock().ok()?.take()
    }

    pub fn is_pending(&self) -> bool {
        self.shared.pending.load(Ordering::Acquire)
    }
}

impl Drop for SearchWorker {
    fn drop(&mut self) {
        self.shared.current_ticket.fetch_add(1, Ordering::AcqRel);
        if let Ok(mut state) = self.shared.request.lock() {
            state.stop = true;
            state.request = None;
            self.shared.request_ready.notify_one();
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn run_worker(shared: Arc<Shared>) {
    loop {
        let request = {
            let mut state = match shared.request.lock() {
                Ok(state) => state,
                Err(_) => return,
            };
            while state.request.is_none() && !state.stop {
                state = match shared.request_ready.wait(state) {
                    Ok(state) => state,
                    Err(_) => return,
                };
            }
            if state.stop {
                return;
            }
            state.request.take().expect("request checked above")
        };

        let hits = search_sources(&request.sources, &request.query, request.index.as_deref(), || {
            shared.current_ticket.load(Ordering::Acquire) != request.ticket
        });
        if shared.current_ticket.load(Ordering::Acquire) != request.ticket {
            continue;
        }
        if let Some(hits) = hits {
            if let Ok(mut result) = shared.result.lock() {
                *result = Some(SearchResponse {
                    query_id: request.query_id,
                    generation: request.generation,
                    hits,
                });
            }
        }
        if shared.current_ticket.load(Ordering::Acquire) == request.ticket {
            shared.pending.store(false, Ordering::Release);
        }
    }
}

/// Search sources without retaining any body. `is_cancelled` is checked before
/// every file and line so a replacement query bounds obsolete work.
pub fn search_sources<F>(sources: &[ContentSearchSource], query: &str, index: Option<&SearchIndex>, mut is_cancelled: F) -> Option<Vec<SearchHit>>
where
    F: FnMut() -> bool,
{
    let query_lower = query.to_lowercase();
    if query_lower.is_empty() {
        return Some(Vec::new());
    }
    let result_limit = if index.is_some() { INDEXED_RESULT_LIMIT } else { FALLBACK_RESULT_LIMIT };
    let indexed_candidates = index.map(|index| candidate_line_count(index, &query_lower)).unwrap_or_default();
    let mut title_order: Vec<usize> = (0..sources.len()).collect();
    title_order.sort_by(|&left, &right| sources[left].title.cmp(&sources[right].title).then_with(|| left.cmp(&right)));
    let mut title_ranks = vec![0usize; sources.len()];
    let mut title_rank = 0usize;
    let mut previous_source: Option<usize> = None;
    for source_index in title_order {
        if previous_source.is_some_and(|previous| sources[previous].title != sources[source_index].title) {
            title_rank += 1;
        }
        title_ranks[source_index] = title_rank;
        previous_source = Some(source_index);
    }
    let mut best_hits = BinaryHeap::with_capacity(indexed_candidates.min(result_limit));
    let mut body = String::new();

    for (source_rank, source) in sources.iter().enumerate() {
        if is_cancelled() {
            return None;
        }
        body.clear();
        let Ok(mut file) = File::open(&source.absolute_path) else {
            continue;
        };
        if file.read_to_string(&mut body).is_err() {
            continue;
        }
        let title_matches = source.title.to_lowercase().contains(&query_lower);
        for (line_number, line) in body.lines().enumerate() {
            if is_cancelled() {
                return None;
            }
            let Some((match_start, match_end)) = match_range(line, &query_lower) else {
                continue;
            };
            let Ok(line_number) = u32::try_from(line_number) else {
                break;
            };
            let mut score = 100;
            if title_matches {
                score += 50;
            }
            if match_start == 0 {
                score += 20;
            }
            if match_start == 0
                || !line
                    .chars()
                    .nth(match_start.saturating_sub(1) as usize)
                    .is_some_and(|character| character.is_alphanumeric())
            {
                score += 10;
            }
            let ranked = RankedHit {
                hit: SearchHit {
                    note_id: source.note_id,
                    line_number,
                    match_start,
                    match_end,
                    score,
                },
                title_rank: title_ranks[source_rank],
                source_rank,
            };
            if best_hits.len() < result_limit {
                best_hits.push(ranked);
            } else if best_hits.peek().is_some_and(|worst| ranked < *worst) {
                best_hits.pop();
                best_hits.push(ranked);
            }
        }
        // `body` is dropped here before the next note is loaded.
    }

    let mut ranked_hits = best_hits.into_vec();
    ranked_hits.sort_by(|left, right| {
        right
            .hit
            .score
            .cmp(&left.hit.score)
            .then_with(|| left.title_rank.cmp(&right.title_rank))
            .then_with(|| left.hit.line_number.cmp(&right.hit.line_number))
            .then_with(|| left.source_rank.cmp(&right.source_rank))
    });
    Some(ranked_hits.into_iter().map(|ranked| ranked.hit).collect())
}

pub fn match_range(line: &str, query_lower: &str) -> Option<(u32, u32)> {
    let line_lower = line.to_lowercase();
    let byte_position = line_lower.find(query_lower)?;
    let start = line_lower[..byte_position].chars().count();
    let end = start.checked_add(query_lower.chars().count())?;
    Some((u32::try_from(start).ok()?, u32::try_from(end).ok()?))
}

fn candidate_line_count(index: &SearchIndex, query: &str) -> usize {
    let mut candidates = index.postings_for_exact(query).len().min(INDEXED_RESULT_LIMIT);
    let mut terms_scanned = 0usize;
    let mut postings_seen = 0usize;
    for (term, postings) in index.postings_for_prefix(query) {
        if term == query {
            continue;
        }
        if terms_scanned >= MAX_PREFIX_TERMS_SCANNED || postings_seen >= MAX_PREFIX_POSTINGS {
            break;
        }
        terms_scanned += 1;
        let posting_count = postings.len().min(50).min(MAX_PREFIX_POSTINGS - postings_seen);
        candidates = candidates.saturating_add(posting_count).min(INDEXED_RESULT_LIMIT);
        postings_seen += posting_count;
    }
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SearchFileFingerprint, SearchSource};
    use std::collections::HashMap;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn unicode_ranges_scores_order_and_limits_are_stable() {
        let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("ekphos-search-query-{unique}"));
        fs::create_dir_all(&root).unwrap();
        let alpha = root.join("alpha.md");
        let other = root.join("other.md");
        fs::write(&alpha, "αlpha at start\nprefix αlpha suffix\n").unwrap();
        fs::write(&other, "some αlpha later\n").unwrap();
        let sources = vec![
            ContentSearchSource {
                note_id: NoteId::new(1),
                title: "αlpha".into(),
                absolute_path: alpha.clone(),
            },
            ContentSearchSource {
                note_id: NoteId::new(2),
                title: "Other".into(),
                absolute_path: other.clone(),
            },
        ];
        let index_sources = vec![
            SearchSource {
                note_id: NoteId::new(1),
                relative_path: "alpha.md".into(),
                absolute_path: alpha,
                fingerprint: SearchFileFingerprint { size: 1, modified_nanos: 1 },
            },
            SearchSource {
                note_id: NoteId::new(2),
                relative_path: "other.md".into(),
                absolute_path: other,
                fingerprint: SearchFileFingerprint { size: 1, modified_nanos: 1 },
            },
        ];
        let index = SearchIndex::build_from_loader(&root, &index_sources, |source| fs::read_to_string(&source.absolute_path).ok().map(Arc::from)).unwrap();
        let fallback = search_sources(&sources, "αLPHA", None, || false).unwrap();
        let indexed = search_sources(&sources, "αLPHA", Some(&index), || false).unwrap();
        assert_eq!(fallback, indexed);
        assert_eq!(indexed.len(), 3);
        assert_eq!(indexed[0].note_id, NoteId::new(1));
        assert_eq!((indexed[0].match_start, indexed[0].match_end), (0, 5));
        assert!(indexed.windows(2).all(|pair| pair[0].score >= pair[1].score));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn differential_corpus_preserves_exact_prefix_substring_unicode_order_and_limits() {
        let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("ekphos-search-differential-{unique}"));
        fs::create_dir_all(root.join("nested")).unwrap();
        let alpha = root.join("Alpha.md");
        let nested = root.join("nested/Other.md");
        fs::write(&alpha, "alpha exact\nalphabet prefix\nxalpha substring\n東京 unicode\ncafé composed\n").unwrap();
        fs::write(&nested, "later alpha occurrence\n東京駅 prefix\n").unwrap();
        let sources = vec![
            ContentSearchSource {
                note_id: NoteId::new(1),
                title: "Alpha".into(),
                absolute_path: alpha.clone(),
            },
            ContentSearchSource {
                note_id: NoteId::new(2),
                title: "Other".into(),
                absolute_path: nested.clone(),
            },
        ];
        let index_sources = vec![
            SearchSource {
                note_id: NoteId::new(1),
                relative_path: "Alpha.md".into(),
                absolute_path: alpha,
                fingerprint: SearchFileFingerprint { size: 1, modified_nanos: 1 },
            },
            SearchSource {
                note_id: NoteId::new(2),
                relative_path: "nested/Other.md".into(),
                absolute_path: nested,
                fingerprint: SearchFileFingerprint { size: 1, modified_nanos: 1 },
            },
        ];
        let index = SearchIndex::build_from_loader(&root, &index_sources, |source| fs::read_to_string(&source.absolute_path).ok().map(Arc::from)).unwrap();

        for query in ["alpha", "alph", "lpha", "東京", "京駅", "CAFÉ"] {
            let expected = legacy_streaming_search(&sources, query, INDEXED_RESULT_LIMIT);
            let actual = search_sources(&sources, query, Some(&index), || false).unwrap();
            assert_eq!(actual, expected, "query {query}");
        }

        let many_path = root.join("Many.md");
        fs::write(&many_path, "needle\n".repeat(600)).unwrap();
        let many = [ContentSearchSource {
            note_id: NoteId::new(3),
            title: "Many".into(),
            absolute_path: many_path.clone(),
        }];
        let many_index_source = [SearchSource {
            note_id: NoteId::new(3),
            relative_path: "Many.md".into(),
            absolute_path: many_path,
            fingerprint: SearchFileFingerprint { size: 1, modified_nanos: 1 },
        }];
        let many_index = SearchIndex::build_from_loader(&root, &many_index_source, |source| {
            fs::read_to_string(&source.absolute_path).ok().map(Arc::from)
        })
        .unwrap();
        assert_eq!(search_sources(&many, "needle", None, || false).unwrap().len(), FALLBACK_RESULT_LIMIT);
        assert_eq!(search_sources(&many, "needle", Some(&many_index), || false).unwrap().len(), 600);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn replacement_cancels_obsolete_work_and_keeps_only_latest_result() {
        let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("ekphos-search-worker-{unique}"));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("note.md");
        fs::write(&path, "alpha\nbeta\n").unwrap();
        let sources: Arc<[ContentSearchSource]> = vec![ContentSearchSource {
            note_id: NoteId::new(7),
            title: "Note".into(),
            absolute_path: path,
        }]
        .into();
        let worker = SearchWorker::new();
        worker.submit(1, 4, "alpha".to_string(), Arc::clone(&sources), None);
        worker.submit(2, 4, "beta".to_string(), sources, None);
        let started = std::time::Instant::now();
        let response = loop {
            if let Some(response) = worker.try_take() {
                break response;
            }
            assert!(started.elapsed() < std::time::Duration::from_secs(2));
            std::thread::yield_now();
        };
        assert_eq!(response.query_id, 2);
        assert_eq!(response.hits.len(), 1);
        let _ = fs::remove_dir_all(root);
    }

    fn legacy_streaming_search(sources: &[ContentSearchSource], query: &str, limit: usize) -> Vec<SearchHit> {
        let query_lower = query.to_lowercase();
        let mut hits = Vec::new();
        for source in sources {
            let body = fs::read_to_string(&source.absolute_path).unwrap();
            let title_matches = source.title.to_lowercase().contains(&query_lower);
            for (line_number, line) in body.lines().enumerate() {
                let Some((match_start, match_end)) = match_range(line, &query_lower) else {
                    continue;
                };
                let chars: Vec<char> = line.chars().collect();
                let mut score = 100;
                if title_matches {
                    score += 50;
                }
                if match_start == 0 {
                    score += 20;
                }
                if match_start == 0
                    || !chars
                        .get(match_start.saturating_sub(1) as usize)
                        .is_some_and(|character| character.is_alphanumeric())
                {
                    score += 10;
                }
                hits.push(SearchHit {
                    note_id: source.note_id,
                    line_number: line_number as u32,
                    match_start,
                    match_end,
                    score,
                });
            }
        }
        let titles: HashMap<_, _> = sources.iter().map(|source| (source.note_id, source.title.as_ref())).collect();
        hits.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| titles.get(&left.note_id).cmp(&titles.get(&right.note_id)))
                .then_with(|| left.line_number.cmp(&right.line_number))
        });
        hits.truncate(limit);
        hits
    }
}
