//! Range fetching over a [`Backend`]: requested ranges are coalesced when
//! the gap between them is small, fetched concurrently, kept in a span
//! cache so no byte is fetched twice, and recorded for the coverage report.

use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use futures_util::stream::{self, StreamExt, TryStreamExt};

use crate::backend::Backend;
use crate::Result;

/// Ranges closer than this are fetched as one request.
pub const COALESCE_GAP: u64 = 16 * 1024;
/// How many requests run at once.
const CONCURRENCY: usize = 8;

/// A fetch observer: called with the offset and length of every request.
pub type FetchObserver = Arc<dyn Fn(u64, u64) + Send + Sync>;

pub struct Fetcher {
    backend: Arc<dyn Backend>,
    len: u64,
    spans: Mutex<BTreeMap<u64, Bytes>>,
    requests: Mutex<Vec<(u64, u64)>>,
    observer: Option<FetchObserver>,
}

impl Fetcher {
    pub async fn new(backend: Arc<dyn Backend>, observer: Option<FetchObserver>) -> Result<Self> {
        let len = backend.len().await?;
        Ok(Self {
            backend,
            len,
            spans: Mutex::new(BTreeMap::new()),
            requests: Mutex::new(Vec::new()),
            observer,
        })
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    /// The `(offset, length)` of every request made so far, in order.
    pub fn requests(&self) -> Vec<(u64, u64)> {
        self.requests.lock().map(|r| r.clone()).unwrap_or_default()
    }

    /// Bytes fetched from the backend so far.
    pub fn bytes_fetched(&self) -> u64 {
        self.requests().iter().map(|(_, len)| len).sum()
    }

    fn clamp(&self, range: &Range<u64>) -> Range<u64> {
        range.start.min(self.len)..range.end.min(self.len)
    }

    /// The bytes of `range` if the span cache covers all of it.
    fn cached(&self, range: &Range<u64>) -> Option<Vec<u8>> {
        let spans = self.spans.lock().ok()?;
        let mut out = Vec::with_capacity((range.end - range.start) as usize);
        let mut at = range.start;
        while at < range.end {
            let (&start, data) = spans.range(..=at).next_back()?;
            let end = start + data.len() as u64;
            if end <= at {
                return None;
            }
            let take_end = end.min(range.end);
            out.extend_from_slice(&data[(at - start) as usize..(take_end - start) as usize]);
            at = take_end;
        }
        Some(out)
    }

    fn missing(&self, range: &Range<u64>) -> Vec<Range<u64>> {
        let Ok(spans) = self.spans.lock() else {
            return vec![range.clone()];
        };
        let mut gaps = Vec::new();
        let mut at = range.start;
        let first = spans
            .range(..=range.start)
            .next_back()
            .map_or(range.start, |(&start, _)| start);
        for (&start, data) in spans.range(first..range.end) {
            let end = start + data.len() as u64;
            if end <= at {
                continue;
            }
            if start > at {
                gaps.push(at..start);
            }
            at = at.max(end);
        }
        if at < range.end {
            gaps.push(at..range.end);
        }
        gaps
    }

    async fn request(&self, range: Range<u64>) -> Result<(u64, Bytes)> {
        let mut data = vec![0u8; (range.end - range.start) as usize];
        let mut filled = 0;
        while filled < data.len() {
            let count = self
                .backend
                .read_at(range.start + filled as u64, &mut data[filled..])
                .await?;
            if count == 0 {
                break;
            }
            filled += count;
        }
        data.truncate(filled);
        if let Some(observer) = &self.observer {
            observer(range.start, filled as u64);
        }
        if let Ok(mut requests) = self.requests.lock() {
            requests.push((range.start, filled as u64));
        }
        Ok((range.start, Bytes::from(data)))
    }

    /// Fetches every range not yet cached, coalescing nearby ones, then
    /// returns the bytes of each requested range in order. A range past
    /// the end of the source comes back short.
    pub async fn fetch(&self, ranges: &[Range<u64>]) -> Result<Vec<Vec<u8>>> {
        let wanted: Vec<Range<u64>> = ranges.iter().map(|r| self.clamp(r)).collect();
        let mut gaps: Vec<Range<u64>> = wanted
            .iter()
            .filter(|r| r.start < r.end)
            .flat_map(|r| self.missing(r))
            .collect();
        gaps.sort_by_key(|r| r.start);
        let merged = coalesce(gaps, COALESCE_GAP, |between| {
            self.missing(between) == [between.clone()]
        });
        let fetched: Vec<(u64, Bytes)> = stream::iter(merged.into_iter().map(|r| self.request(r)))
            .buffer_unordered(CONCURRENCY)
            .try_collect()
            .await?;
        if let Ok(mut spans) = self.spans.lock() {
            fetched
                .into_iter()
                .for_each(|(start, data)| insert_span(&mut spans, start, data));
        }
        Ok(wanted
            .iter()
            .map(|r| {
                if r.start >= r.end {
                    return Vec::new();
                }
                self.cached(r).unwrap_or_default()
            })
            .collect())
    }

    /// An image of the whole source holding every byte fetched so far at
    /// its offset, zeros elsewhere.
    pub fn image(&self) -> Vec<u8> {
        let mut image = vec![0u8; usize::try_from(self.len).unwrap_or(0)];
        if let Ok(spans) = self.spans.lock() {
            for (&start, data) in spans.iter() {
                let start = start as usize;
                let end = (start + data.len()).min(image.len());
                image[start..end].copy_from_slice(&data[..end - start]);
            }
        }
        image
    }

    pub async fn fetch_one(&self, range: Range<u64>) -> Result<Vec<u8>> {
        Ok(self.fetch(&[range]).await?.pop().unwrap_or_default())
    }
}

/// Inserts a span, merging it with every span it overlaps or touches, so
/// the cache stays a set of disjoint spans and a lookup finds the one span
/// covering an offset.
fn insert_span(spans: &mut BTreeMap<u64, Bytes>, start: u64, data: Bytes) {
    let end = start + data.len() as u64;
    let first = spans
        .range(..=start)
        .next_back()
        .filter(|(&s, d)| s + d.len() as u64 >= start)
        .map_or(start, |(&s, _)| s);
    let touching: Vec<u64> = spans.range(first..=end).map(|(&s, _)| s).collect();
    if touching.is_empty() {
        spans.insert(start, data);
        return;
    }
    let merged_end = touching
        .iter()
        .filter_map(|s| spans.get(s).map(|d| s + d.len() as u64))
        .fold(end, u64::max);
    let merged_start = first.min(start);
    let mut merged = vec![0u8; (merged_end - merged_start) as usize];
    for s in &touching {
        if let Some(old) = spans.remove(s) {
            let at = (s - merged_start) as usize;
            merged[at..at + old.len()].copy_from_slice(&old);
        }
    }
    let at = (start - merged_start) as usize;
    merged[at..at + data.len()].copy_from_slice(&data);
    spans.insert(merged_start, Bytes::from(merged));
}

/// Merges sorted ranges whose gaps are at most `gap` bytes, when `mergeable`
/// accepts the bytes between them (the fetcher refuses cached bytes, so
/// nothing is fetched twice).
pub fn coalesce(
    sorted: Vec<Range<u64>>,
    gap: u64,
    mergeable: impl Fn(&Range<u64>) -> bool,
) -> Vec<Range<u64>> {
    let mut out: Vec<Range<u64>> = Vec::with_capacity(sorted.len());
    for range in sorted {
        if let Some(last) = out.last_mut() {
            let between = last.end..range.start;
            if range.start <= last.end
                || range.start <= last.end.saturating_add(gap) && mergeable(&between)
            {
                last.end = last.end.max(range.end);
                continue;
            }
        }
        out.push(range);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::MemBackend;

    #[test]
    fn coalesce_merges_close_ranges_only() {
        let merged = coalesce(vec![0..10, 12..20, 100..110, 105..130], 4, |_| true);
        assert_eq!(merged, vec![0..20, 100..130]);
        let kept = coalesce(vec![0..10, 12..20], 4, |_| false);
        assert_eq!(kept, vec![0..10, 12..20]);
    }

    #[tokio::test]
    async fn cached_bytes_are_not_fetched_twice() {
        let data: Vec<u8> = (0..=255u8).cycle().take(200_000).collect();
        let fetcher = Fetcher::new(Arc::new(MemBackend::from(data.clone())), None)
            .await
            .unwrap();
        let first = fetcher.fetch(&[10..20, 150_000..150_010]).await.unwrap();
        assert_eq!(first[0], data[10..20]);
        assert_eq!(first[1], data[150_000..150_010]);
        let requests = fetcher.requests().len();
        let again = fetcher.fetch_one(12..18).await.unwrap();
        assert_eq!(again, data[12..18]);
        assert_eq!(fetcher.requests().len(), requests);
        let spanning = fetcher.fetch_one(15..40).await.unwrap();
        assert_eq!(spanning, data[15..40]);
        assert_eq!(fetcher.bytes_fetched(), 10 + 10 + 20);
    }

    #[test]
    fn spans_merge_into_disjoint_runs() {
        let mut spans = BTreeMap::new();
        insert_span(&mut spans, 10, Bytes::from_static(b"aaaa"));
        insert_span(&mut spans, 20, Bytes::from_static(b"bbbb"));
        insert_span(&mut spans, 12, Bytes::from_static(b"cccccccccc"));
        assert_eq!(spans.len(), 1);
        assert_eq!(&spans[&10][..], b"aaccccccccccbb");
        insert_span(&mut spans, 22, Bytes::from_static(b"dd"));
        assert_eq!(spans.len(), 1);
        insert_span(&mut spans, 40, Bytes::from_static(b"e"));
        assert_eq!(spans.len(), 2);
    }
}
