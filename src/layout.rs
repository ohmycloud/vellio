//! Legacy normalized page regions shared by drawing and reverse navigation.
use crate::model::ReaderState;
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageBox(pub [f32; 4]);

impl PageBox {
    fn parse(value: &Value) -> Option<Self> {
        let values = value.as_array()?;
        if values.len() != 4 {
            return None;
        }
        let mut bounds = [0.; 4];
        for (i, value) in values.iter().enumerate() {
            let coordinate = value.as_f64()?;
            if !coordinate.is_finite() {
                return None;
            }
            bounds[i] = coordinate.clamp(0., 1.) as f32;
        }
        (bounds[0] < bounds[2] && bounds[1] < bounds[3]).then_some(Self(bounds))
    }

    fn contains(self, x: f32, y: f32) -> bool {
        let [x0, y0, x1, y1] = self.0;
        x >= x0 && x <= x1 && y >= y0 && y <= y1
    }

    fn area(self) -> f32 {
        let [x0, y0, x1, y1] = self.0;
        (x1 - x0) * (y1 - y0)
    }
}

#[derive(Clone, Debug)]
pub struct Region {
    pub block: usize,
    pub page: usize,
    pub boxes: Vec<PageBox>,
}

#[derive(Clone, Debug, Default)]
pub struct PageLayout {
    regions: Vec<Region>,
}

impl PageLayout {
    pub fn from_state(raw: &Value, state: &ReaderState) -> Self {
        let mut regions = Vec::new();
        for (index, block) in state.paper.blocks.iter().enumerate() {
            let location = &raw["layout"][&block.id];
            let page = location["page"]
                .as_u64()
                .and_then(|p| usize::try_from(p).ok())
                .filter(|p| *p > 0)
                .unwrap_or(block.page);
            if page == 0 || page > state.paper.meta.pages.len() {
                continue;
            }
            // Separate column boxes take precedence over their union. The
            // gutter is neither highlighted nor selectable as this block.
            let mut boxes: Vec<_> = location["boxes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(PageBox::parse)
                .collect();
            if boxes.is_empty() {
                boxes.extend(
                    PageBox::parse(&location["box"])
                        .or_else(|| PageBox::parse(&raw["paper"]["blocks"][index]["box"])),
                );
            }
            if !boxes.is_empty() {
                regions.push(Region {
                    block: index,
                    page,
                    boxes,
                });
            }
        }
        Self { regions }
    }

    pub fn region(&self, block: usize) -> Option<&Region> {
        self.regions.iter().find(|r| r.block == block)
    }

    /// Prefer the smallest actual region when headings/figures overlap a
    /// whole-page fallback. Reject points outside the displayed page.
    pub fn block_at(&self, page: usize, x: f32, y: f32) -> Option<usize> {
        if !x.is_finite()
            || !y.is_finite()
            || !(0.0..=1.0).contains(&x)
            || !(0.0..=1.0).contains(&y)
        {
            return None;
        }
        self.regions
            .iter()
            .filter(|r| r.page == page)
            .flat_map(|r| r.boxes.iter().map(move |b| (r.block, *b)))
            .filter(|(_, b)| b.contains(x, y))
            .min_by(|a, b| a.1.area().total_cmp(&b.1.area()))
            .map(|(block, _)| block)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn columns_remain_separate_and_overlap_picks_smallest_region() {
        let raw = json!({"paper":{"meta":{"pages":[{},{}]},"blocks":[
            {"id":"page","type":"para","page":1},
            {"id":"columns","type":"para","page":1},
            {"id":"heading","type":"heading","page":2}
        ]},"layout":{
            "page":{"page":1,"box":[0,0,1,1]},
            "columns":{"page":1,"box":[0.1,0.2,0.9,0.8],"boxes":[[0.1,0.2,0.4,0.8],[0.6,0.2,0.9,0.8]]},
            "heading":{"page":1,"box":[0.12,0.22,0.3,0.3]},
            "unknown":{"page":1,"box":[0,0,1,1]}
        }});
        let state = ReaderState::from_value(raw.clone()).unwrap();
        let layout = PageLayout::from_state(&raw, &state);
        assert_eq!(layout.region(1).unwrap().boxes.len(), 2);
        assert_eq!(layout.region(2).unwrap().page, 1);
        assert_eq!(layout.block_at(1, 0.2, 0.25), Some(2));
        assert_eq!(layout.block_at(1, 0.2, 0.6), Some(1));
        assert_eq!(layout.block_at(1, 0.8, 0.6), Some(1));
        assert_eq!(layout.block_at(1, 0.5, 0.6), Some(0));
        assert_eq!(layout.block_at(2, 0.2, 0.6), None);
        assert_eq!(layout.block_at(1, -0.1, 0.6), None);
    }

    #[test]
    fn invalid_geometry_is_ignored_and_legacy_manual_boxes_are_supported() {
        let raw = json!({"paper":{"meta":{"pages":[{}]},"blocks":[
            {"id":"manual","type":"figure","page":1,"box":[-0.1,0.1,1.1,0.5]},
            {"id":"bad","type":"para","page":1},
            {"id":"wrong-page","type":"para","page":1}
        ]},"layout":{
            "manual":{"boxes":[[0.5,0.3,0.2,0.4],["bad",0,1,1]]},
            "bad":{"page":1,"box":[1,0,0,1]},
            "wrong-page":{"page":99,"box":[0,0,1,1]}
        }});
        let state = ReaderState::from_value(raw.clone()).unwrap();
        let layout = PageLayout::from_state(&raw, &state);
        assert_eq!(
            layout.region(0).unwrap().boxes,
            vec![PageBox([0., 0.1, 1., 0.5])]
        );
        assert!(layout.region(1).is_none());
        assert!(layout.region(2).is_none());
        assert_eq!(layout.block_at(1, 0.5, 0.8), None);
        assert_eq!(layout.block_at(1, f32::NAN, 0.3), None);
    }
}
