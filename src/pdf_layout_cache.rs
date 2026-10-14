//! Disposable PDF presentation cache and a bounded, demand-first preparation plan.
//! Neither extracted book text nor reader-owned records are rewritten.
use crate::pdf::{self, CodePresentation};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

// Bump when PDF recovery semantics change; old layouts must not be reused.
// v12 also recovers captioned tables with a single header rule and vertical
// separators; rebuild v11 presentations that misclassified them as images.
const VERSION: u32 = 13;
pub const LOOKAHEAD_PAGES: usize = 16;
pub const TRAILING_PAGES: usize = 4;
pub const BATCH_SIZE: usize = 4;

#[derive(Clone)]
pub struct PresentationCache {
    directory: PathBuf,
}

pub enum PreparedPage {
    OnDisk,
    Loaded(Option<CodePresentation>),
}

#[derive(Serialize, Deserialize)]
struct Record {
    version: u32,
    page: usize,
    source: String,
    presentation: Option<CodePresentation>,
}

impl PresentationCache {
    pub fn new(pdf: &Path, root: &Path) -> Result<Self> {
        let metadata = fs::metadata(pdf)?;
        let modified = metadata.modified()?.duration_since(UNIX_EPOCH)?;
        let identity = format!(
            "{}:{}:{}:{}",
            pdf.canonicalize()?.display(),
            metadata.len(),
            modified.as_secs(),
            modified.subsec_nanos()
        );
        let digest = format!("{:x}", Sha256::digest(identity.as_bytes()));
        Ok(Self {
            directory: root.join(format!("v{VERSION}-{digest}")),
        })
    }

    fn path(&self, page: usize, source: &str) -> PathBuf {
        self.directory.join(format!(
            "p{page}-{:x}.json",
            Sha256::digest(source.as_bytes())
        ))
    }

    fn read(&self, page: usize, source: &str) -> Option<Option<CodePresentation>> {
        let record: Record =
            serde_json::from_slice(&fs::read(self.path(page, source)).ok()?).ok()?;
        if record.version != VERSION || record.page != page || record.source != source {
            return None;
        }
        if let Some(presentation) = &record.presentation {
            if presentation.math.iter().any(|m| !m.layout.is_valid()) {
                return None;
            }
            crate::typst_table::warm_html(&presentation.html).ok()?;
            for math in &presentation.math {
                crate::formula::publish_layout(&math.tex, math.display, math.layout.clone());
            }
        }
        Some(record.presentation)
    }

    fn write(
        &self,
        page: usize,
        source: &str,
        presentation: &Option<CodePresentation>,
    ) -> Result<()> {
        fs::create_dir_all(&self.directory)?;
        let mut file = tempfile::NamedTempFile::new_in(&self.directory)?;
        serde_json::to_writer(
            &mut file,
            &Record {
                version: VERSION,
                page,
                source: source.to_owned(),
                presentation: presentation.clone(),
            },
        )?;
        // A cache is rebuildable; atomic replacement matters, fsync per page
        // would needlessly hold up preparation of the next visible page.
        file.persist(self.path(page, source)).map_err(|e| e.error)?;
        Ok(())
    }

    pub fn prepare(
        &self,
        pdf: &Path,
        requests: &[(usize, String)],
    ) -> Vec<Result<Option<CodePresentation>>> {
        self.prepare_with(requests, |missing| pdf::code_presentations(pdf, missing))
    }

    /// Background work need not decode cached raster previews for pages far
    /// from the viewport. Validate/rebuild those payloads when actually needed.
    pub fn prepare_selective(
        &self,
        pdf: &Path,
        requests: &[(usize, String, bool)],
    ) -> Vec<Result<PreparedPage>> {
        let mut results: Vec<_> = requests
            .iter()
            .map(|(page, source, needed)| {
                (!needed && self.path(*page, source).is_file()).then_some(Ok(PreparedPage::OnDisk))
            })
            .collect();
        let missing: Vec<_> = results
            .iter()
            .enumerate()
            .filter_map(|(i, r)| r.is_none().then_some(i))
            .collect();
        let inputs: Vec<_> = missing
            .iter()
            .map(|i| (requests[*i].0, requests[*i].1.clone()))
            .collect();
        for (i, result) in missing.into_iter().zip(self.prepare(pdf, &inputs)) {
            results[i] = Some(result.map(PreparedPage::Loaded));
        }
        results
            .into_iter()
            .map(|r| r.expect("every page has a preparation result"))
            .collect()
    }

    fn prepare_with(
        &self,
        requests: &[(usize, String)],
        load: impl FnOnce(&[(usize, String)]) -> Result<Vec<Result<Option<CodePresentation>>>>,
    ) -> Vec<Result<Option<CodePresentation>>> {
        let mut results: Vec<_> = requests
            .iter()
            .map(|(page, source)| self.read(*page, source).map(Ok))
            .collect();
        let missing: Vec<_> = results
            .iter()
            .enumerate()
            .filter_map(|(i, result)| result.is_none().then_some(i))
            .collect();
        if !missing.is_empty() {
            let inputs: Vec<_> = missing.iter().map(|i| requests[*i].clone()).collect();
            let fresh = match load(&inputs) {
                Ok(results) => results,
                Err(error) => inputs
                    .iter()
                    .map(|_| Err(anyhow::anyhow!("{error:#}")))
                    .collect(),
            };
            let mut fresh = fresh.into_iter();
            for i in missing {
                let result = fresh
                    .next()
                    .context("Missing PDF batch result")
                    .and_then(|r| r);
                if let Ok(presentation) = &result {
                    // Unwritable/corrupt cache files must never block reading.
                    let _ = self.write(requests[i].0, &requests[i].1, presentation);
                }
                results[i] = Some(result);
            }
        }
        results
            .into_iter()
            .map(|r| r.expect("every requested page has a result"))
            .collect()
    }
}

/// Current page and reading-direction lookahead first, then the rest of the
/// book. Recompute between batches; never leave hundreds of PDF-lock waiters.
pub fn lookahead(eligible: &[usize], focus: usize, forward: bool) -> Vec<usize> {
    let pivot = eligible.partition_point(|index| *index < focus);
    if forward {
        eligible[pivot..]
            .iter()
            .take(LOOKAHEAD_PAGES + 1)
            .chain(eligible[..pivot].iter().rev().take(TRAILING_PAGES))
            .copied()
            .collect()
    } else {
        eligible
            .get(pivot)
            .into_iter()
            .chain(eligible[..pivot].iter().rev().take(LOOKAHEAD_PAGES))
            .chain(
                eligible[pivot.saturating_add(1).min(eligible.len())..]
                    .iter()
                    .take(TRAILING_PAGES),
            )
            .copied()
            .collect()
    }
}

pub fn next_batch(
    eligible: &[usize],
    focus: usize,
    forward: bool,
    ready: &HashSet<usize>,
    prepared: &HashSet<usize>,
) -> Vec<usize> {
    let pivot = eligible.partition_point(|index| *index < focus);
    let nearby = lookahead(eligible, focus, forward);
    let mut batch = Vec::new();
    for index in &nearby {
        if !ready.contains(index) {
            batch.push(*index);
            if batch.len() == BATCH_SIZE {
                return batch;
            }
        }
    }
    let background: Vec<_> = if forward {
        eligible[pivot..]
            .iter()
            .chain(eligible[..pivot].iter())
            .copied()
            .collect()
    } else {
        eligible[..pivot]
            .iter()
            .rev()
            .chain(eligible[pivot..].iter())
            .copied()
            .collect()
    };
    for index in &background {
        if !ready.contains(index) && !prepared.contains(index) && !batch.contains(index) {
            batch.push(*index);
            if batch.len() == BATCH_SIZE {
                break;
            }
        }
    }
    batch
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::FigurePreview;

    fn cache(root: &Path) -> PresentationCache {
        let pdf = root.join("source.pdf");
        fs::write(&pdf, b"identity fixture, not opened by these tests").unwrap();
        PresentationCache::new(&pdf, &root.join("cache")).unwrap()
    }

    #[test]
    fn cached_pages_and_plain_fallback_survive_reopening_without_pdf_work() {
        let root = tempfile::tempdir().unwrap();
        let cache = cache(root.path());
        let requests = vec![(1, "code".into()), (2, "plain".into())];
        let result = cache.prepare_with(&requests, |_| {
            Ok(vec![
                Ok(Some(CodePresentation {
                    html: "<pre>code</pre>".into(),
                    math: vec![crate::pdf::MathPresentation {
                        tex: "cache-test-equation".into(),
                        display: true,
                        number: Some("1".into()),
                        layout: crate::formula::FormulaLayout {
                            width: 120.,
                            height: 30.,
                            baseline: 20.,
                            paths: vec![crate::formula::FormulaPath {
                                stroke: Some(0.5),
                                commands: vec![
                                    crate::formula::PathCommand::Move([1., 20.]),
                                    crate::formula::PathCommand::Line([119., 20.]),
                                ],
                            }],
                        },
                    }],
                    inline_figures: vec!["figure".into()],
                    figures: vec![FigurePreview {
                        id: "figure".into(),
                        png: vec![0, 1, 255],
                        width: 2,
                        height: 3,
                    }],
                })),
                Ok(None),
            ])
        });
        assert!(result[0].as_ref().unwrap().is_some());
        let reopened =
            PresentationCache::new(&root.path().join("source.pdf"), &root.path().join("cache"))
                .unwrap();
        let result = reopened.prepare_with(&requests, |_| panic!("cache hit must not open PDFium"));
        let presentation = result[0].as_ref().unwrap().as_ref().unwrap();
        assert_eq!(presentation.figures[0].png, vec![0, 1, 255]);
        assert_eq!(presentation.math[0].tex, "cache-test-equation");
        assert_eq!(
            (
                presentation.math[0].layout.width,
                presentation.math[0].layout.height
            ),
            (120., 30.)
        );
        assert!(crate::formula::cached_layout("cache-test-equation", true).is_some());
        assert_eq!(presentation.inline_figures, ["figure"]);
        assert!(result[1].as_ref().unwrap().is_none());
    }

    #[test]
    fn changed_source_pdf_and_corrupt_cache_are_rebuilt() {
        let root = tempfile::tempdir().unwrap();
        let cache = cache(root.path());
        cache.write(1, "old", &None).unwrap();
        assert!(cache.read(1, "new").is_none());
        fs::write(cache.path(1, "old"), b"broken").unwrap();
        assert!(cache.read(1, "old").is_none());
        cache.write(1, "old", &None).unwrap();
        fs::write(root.path().join("source.pdf"), b"changed PDF identity").unwrap();
        let changed =
            PresentationCache::new(&root.path().join("source.pdf"), &root.path().join("cache"))
                .unwrap();
        assert!(changed.read(1, "old").is_none());
    }

    #[test]
    fn a_failed_page_does_not_poison_cached_siblings_or_become_a_cache_hit() {
        let root = tempfile::tempdir().unwrap();
        let cache = cache(root.path());
        let requests = vec![(1, "valid".into()), (2, "failed".into())];
        let result = cache.prepare_with(&requests, |_| {
            Ok(vec![Ok(None), Err(anyhow::anyhow!("bad page"))])
        });
        assert!(result[1].is_err());
        assert_eq!(cache.read(1, "valid").map(|p| p.is_none()), Some(true));
        assert!(cache.read(2, "failed").is_none());
    }

    #[test]
    fn background_cache_hits_do_not_decode_preview_payloads() {
        let root = tempfile::tempdir().unwrap();
        let cache = cache(root.path());
        cache.write(1, "source", &None).unwrap();
        // For an offscreen page, existence is only a cheap scheduling hint.
        // Even a corrupt payload needn't be decoded until it is requested.
        fs::write(cache.path(1, "source"), b"broken").unwrap();
        let result = cache.prepare_selective(
            &root.path().join("missing.pdf"),
            &[(1, "source".into(), false)],
        );
        assert!(matches!(result[0], Ok(PreparedPage::OnDisk)));
        let result = cache.prepare_selective(
            &root.path().join("missing.pdf"),
            &[(1, "source".into(), true)],
        );
        assert!(
            result[0].is_err(),
            "demanded corrupt payload must be rebuilt"
        );
    }

    #[test]
    fn scheduling_reprioritizes_jumps_and_prepares_beyond_the_two_page_horizon() {
        let eligible: Vec<_> = (0..100).collect();
        let empty = HashSet::new();
        assert_eq!(
            next_batch(&eligible, 40, true, &empty, &empty),
            [40, 41, 42, 43]
        );
        let ready: HashSet<_> = (40..44).collect();
        assert_eq!(
            next_batch(&eligible, 40, true, &ready, &ready),
            [44, 45, 46, 47]
        );
        assert_eq!(
            next_batch(&eligible, 80, true, &ready, &ready),
            [80, 81, 82, 83]
        );
        assert_eq!(
            next_batch(&eligible, 30, false, &ready, &ready),
            [30, 29, 28, 27]
        );
        let ready: HashSet<_> = (36..57).collect();
        assert_eq!(
            next_batch(&eligible, 40, true, &ready, &ready),
            [57, 58, 59, 60]
        );
        let all: HashSet<_> = eligible.iter().copied().collect();
        assert!(next_batch(&eligible, 40, true, &all, &all).is_empty());
    }
}
