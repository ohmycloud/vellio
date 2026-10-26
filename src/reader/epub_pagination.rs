//! Reflow pages are a view of measured content, never source/PDF page numbers.
//! All offsets are in layout pixels within the original, unsplit content block.
#[derive(Clone, Debug)]
pub struct MeasuredBlock {
    pub height: f32,
    /// Legal cut positions (actual glyph-row boundaries, not character counts).
    pub breaks: Vec<f32>,
    pub chapter: usize,
    pub heading: bool,
    pub atomic: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Fragment {
    pub block: usize,
    pub start: f32,
    pub end: f32,
}

pub type Page = Vec<Fragment>;

pub fn paginate(blocks: &[MeasuredBlock], page_height: f32) -> Vec<Page> {
    if !page_height.is_finite() || page_height <= 0.0 {
        return Vec::new();
    }
    let mut pages = Vec::new();
    let mut page = Vec::new();
    let mut used = 0.0;
    for (index, block) in blocks.iter().enumerate() {
        if index > 0 && block.chapter != blocks[index - 1].chapter && !page.is_empty() {
            pages.push(std::mem::take(&mut page));
            used = 0.0;
        }
        let height = block.height;
        if !height.is_finite() || height <= 0.0 {
            continue;
        }
        // Keep small blocks/images and headings with the first row of their body.
        // Ordinary multi-line paragraphs may use the remainder of a page.
        let next_row = blocks
            .get(index + 1)
            .filter(|next| next.chapter == block.chapter)
            .map(|next| next.breaks.first().copied().unwrap_or(next.height))
            .unwrap_or(0.0);
        let keep = if block.heading {
            height + next_row
        } else {
            height
        };
        if !page.is_empty()
            && (block.atomic || block.heading || height <= page_height * 0.25)
            && keep.min(page_height) > page_height - used + 0.01
        {
            pages.push(std::mem::take(&mut page));
            used = 0.0;
        }
        let mut cuts: Vec<_> = block
            .breaks
            .iter()
            .copied()
            .filter(|cut| cut.is_finite() && *cut > 0.0 && *cut < height)
            .collect();
        cuts.sort_by(f32::total_cmp);
        cuts.dedup_by(|a, b| (*a - *b).abs() < 0.01);
        cuts.push(height);
        let mut start = 0.0;
        while start < height - 0.01 {
            let limit = start + page_height - used;
            let end = cuts
                .iter()
                .copied()
                .rev()
                .find(|cut| *cut > start + 0.01 && *cut <= limit + 0.01);
            let end = match end {
                Some(end) => end,
                None if !page.is_empty() => {
                    pages.push(std::mem::take(&mut page));
                    used = 0.0;
                    continue;
                }
                // Oversized atomic media: advance geometrically rather than loop
                // forever. The desktop scales figures to avoid this case.
                None => (start + page_height).min(height),
            };
            page.push(Fragment {
                block: index,
                start,
                end,
            });
            used += end - start;
            start = end;
            if start < height - 0.01 {
                pages.push(std::mem::take(&mut page));
                used = 0.0;
            }
        }
    }
    if !page.is_empty() {
        pages.push(page);
    }
    pages
}

pub fn page_for_anchor(pages: &[Page], block: usize, offset: f32) -> Option<usize> {
    pages
        .iter()
        .position(|page| {
            page.iter().any(|fragment| {
                fragment.block == block && offset >= fragment.start && offset < fragment.end
            })
        })
        .or_else(|| {
            pages
                .iter()
                .position(|page| page.iter().any(|f| f.block == block))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn block(height: f32, step: f32) -> MeasuredBlock {
        MeasuredBlock {
            height,
            breaks: (1..=(height / step) as usize)
                .map(|i| i as f32 * step)
                .collect(),
            chapter: 1,
            heading: false,
            atomic: false,
        }
    }
    #[test]
    fn long_paragraph_is_split_on_real_lines_without_loss_or_overlap() {
        let pages = paginate(&[block(525.0, 35.0)], 200.0);
        assert_eq!(
            pages.iter().map(|p| p[0].end).collect::<Vec<_>>(),
            [175.0, 350.0, 525.0]
        );
        assert_eq!(page_for_anchor(&pages, 0, 175.0), Some(1));
        assert_eq!(
            pages.iter().flatten().map(|f| f.end - f.start).sum::<f32>(),
            525.0
        );
    }
    #[test]
    fn short_blocks_headings_and_chapter_starts_are_kept_together() {
        let mut heading = block(40.0, 40.0);
        heading.heading = true;
        let mut chapter = block(20.0, 20.0);
        chapter.chapter = 2;
        let pages = paginate(
            &[block(160.0, 20.0), heading, block(100.0, 20.0), chapter],
            200.0,
        );
        assert_eq!(pages.len(), 3);
        assert_eq!(pages[1].iter().map(|f| f.block).collect::<Vec<_>>(), [1, 2]);
        assert_eq!(pages[2][0].block, 3);
    }
    #[test]
    fn resize_reflows_and_anchor_follows_the_same_content() {
        let blocks = [block(600.0, 30.0)];
        let small = paginate(&blocks, 180.0);
        let large = paginate(&blocks, 300.0);
        assert_eq!(small.len(), 4);
        assert_eq!(large.len(), 2);
        assert_eq!(page_for_anchor(&small, 0, 360.0), Some(2));
        assert_eq!(page_for_anchor(&large, 0, 360.0), Some(1));
        assert!(paginate(&blocks, 0.0).is_empty());
    }
    #[test]
    fn prose_uses_remaining_space_instead_of_treating_every_paragraph_as_a_page() {
        let pages = paginate(&[block(120.0, 20.0), block(120.0, 20.0)], 200.0);
        assert_eq!(pages[0].len(), 2);
        assert_eq!(
            pages[0][1],
            Fragment {
                block: 1,
                start: 0.0,
                end: 80.0
            }
        );
        assert_eq!(
            pages[1][0],
            Fragment {
                block: 1,
                start: 80.0,
                end: 120.0
            }
        );
    }
    #[test]
    fn oversized_media_and_empty_books_terminate() {
        let mut image = block(650.0, 650.0);
        image.atomic = true;
        let pages = paginate(&[image], 200.0);
        assert_eq!(pages.len(), 4);
        assert_eq!(pages.last().unwrap()[0].end, 650.0);
        assert!(paginate(&[], 200.0).is_empty());
    }
}
