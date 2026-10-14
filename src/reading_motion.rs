//! Smooth logical scrolling across a virtual list with unequal row heights.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollPosition {
    pub block: usize,
    pub offset: f32,
}

pub struct ScrollPath {
    starts: Vec<f32>,
    from: ScrollPosition,
    to: ScrollPosition,
    from_px: f32,
    to_px: f32,
}

impl ScrollPath {
    pub fn new(mut heights: Vec<f32>, from: ScrollPosition, to: ScrollPosition) -> Self {
        assert!(!heights.is_empty());
        for height in &mut heights {
            if !height.is_finite() || *height < 1. {
                *height = 1.;
            }
        }
        for position in [from, to] {
            heights[position.block] = heights[position.block].max(position.offset + 1.);
        }
        let mut starts = vec![0.];
        for height in heights {
            starts.push(starts.last().unwrap() + height);
        }
        Self {
            from_px: starts[from.block] + from.offset,
            to_px: starts[to.block] + to.offset,
            starts,
            from,
            to,
        }
    }

    pub fn sample(&self, progress: f32) -> ScrollPosition {
        if !progress.is_finite() || progress <= 0. {
            return self.from;
        }
        if progress >= 1. {
            return self.to;
        }
        let eased = progress * progress * (3. - 2. * progress);
        let y = self.from_px + (self.to_px - self.from_px) * eased;
        let block = self
            .starts
            .partition_point(|start| *start <= y)
            .saturating_sub(1)
            .min(self.starts.len() - 2);
        ScrollPosition {
            block,
            offset: (y - self.starts[block]).max(0.),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smooth_scroll_preserves_endpoints_and_crosses_unequal_rows_in_both_directions() {
        let start = ScrollPosition {
            block: 0,
            offset: 30.,
        };
        let end = ScrollPosition {
            block: 2,
            offset: 90.,
        };
        let heights = vec![100., 900., 200.];
        for (from, to) in [(start, end), (end, start)] {
            let path = ScrollPath::new(heights.clone(), from, to);
            assert_eq!(path.sample(0.), from);
            assert_eq!(path.sample(1.), to);
            assert_eq!(path.sample(f32::NAN), from);
            let mut previous = path.from_px;
            for frame in 1..=60 {
                let point = path.sample(frame as f32 / 60.);
                let current = path.starts[point.block] + point.offset;
                assert!(if from.block < to.block {
                    current >= previous
                } else {
                    current <= previous
                });
                previous = current;
            }
            assert_eq!(path.sample(0.5).block, 1);
        }
        let path = ScrollPath::new(
            vec![100.],
            start,
            ScrollPosition {
                block: 0,
                offset: 70.,
            },
        );
        assert_eq!(path.sample(0.5).offset, 50.);
    }
}
