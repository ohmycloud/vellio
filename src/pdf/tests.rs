#[test]
fn clipped_callout_backgrounds_do_not_absorb_prose_or_real_figures() {
    assert!(super::extends_across_page(
        [72., -2365., 540., 401.4],
        612.,
        792.
    ));
    assert!(super::extends_across_page(
        [72., -1717., 540., 1049.],
        612.,
        792.
    ));
    assert!(super::extends_across_page(
        [72., -1717., 72.6, 1049.],
        612.,
        792.
    ));
    assert!(!super::extends_across_page(
        [93., 292., 519., 696.],
        612.,
        792.
    ));
    assert!(!super::extends_across_page(
        [0., 0., 612., 792.],
        612.,
        792.
    ));
    assert!(!super::extends_across_page(
        [72., -40., 540., 600.],
        612.,
        792.
    ));
}
use super::*;
#[test]
fn chart_labels_expand_crop_without_following_surrounding_prose() {
    assert_eq!(
        include_labels(
            [100, 100, 600, 500],
            &[
                [65, 180, 82, 400],   // Rotated vertical axis label.
                [240, 520, 460, 540], // Horizontal axis label.
                [240, 568, 460, 590], // Do not walk out through the first label.
                [60, 60, 700, 82],    // A wider prose line is not part of the chart.
            ]
        ),
        [65, 100, 600, 540]
    );
}
#[test]
fn fragmented_prose_below_a_wide_figure_is_not_a_label() {
    let bounds = [100, 100, 1000, 300];
    // A body line at exactly the same left margin, made of tiny PDF runs.
    let mut labels: Vec<_> = (0..85)
        .map(|i| [100 + i * 10, 325, 108 + i * 10, 340])
        .collect();
    labels.extend([[420, 80, 600, 95], [50, 130, 80, 250]]);
    assert_eq!(include_labels(bounds, &labels), [50, 80, 1000, 300]);
    labels.reverse();
    assert_eq!(include_labels(bounds, &labels), [50, 80, 1000, 300]);
}
#[test]
fn shared_captions_group_rows_not_stacks_or_independently_captioned_art() {
    let panels = [
        (2, [100, 100, 220, 300], true),
        (4, [320, 90, 480, 330], true),
        (6, [580, 100, 700, 300], false),
        (8, [100, 600, 700, 800], true),
    ];
    let groups = captioned_regions(&panels, &[[80, 350, 720, 390]]);
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[1].indices, [2, 4, 6]);
    assert_eq!(groups[1].bounds, [100, 90, 700, 330]);
    assert_eq!(captioned_regions(&panels, &[]).len(), 4);
    assert_eq!(
        captioned_regions(
            &panels,
            &[
                [100, 350, 230, 370],
                [320, 350, 470, 370],
                [580, 350, 700, 370]
            ]
        )
        .len(),
        4
    );
    // A short centered shared caption also identifies two panels.
    assert_eq!(
        captioned_regions(&panels[..2], &[[270, 350, 310, 370]]).len(),
        1
    );
    // A later, wider caption must not absorb a panel with its own label.
    assert_eq!(
        captioned_regions(&panels[..2], &[[100, 310, 230, 330], [80, 350, 720, 390]]).len(),
        2
    );
    assert_eq!(
        captioned_regions(&panels[..2], &[[320, 340, 480, 345], [80, 350, 720, 390]]).len(),
        2
    );
    assert_eq!(
        captioned_regions(&[panels[0], panels[3]], &[[80, 820, 720, 840]]).len(),
        2
    );
}

#[test]
fn shared_panel_titles_do_not_walk_into_prose() {
    let groups = captioned_regions(
        &[
            (1, [180, 100, 300, 300], true),
            (3, [500, 100, 700, 330], true),
        ],
        &[[150, 350, 720, 370]],
    );
    assert_eq!(groups.len(), 1);
    assert_eq!(
        groups[0].with_labels(&[
            [130, 70, 350, 85], // Wider than the left bitmap, centered on it.
            [520, 70, 680, 85],
            [80, 40, 780, 55],  // Broad preceding body, not a title.
            [130, 10, 350, 25], // Must not walk upward from the first title.
        ]),
        [130, 70, 700, 330]
    );
}

#[test]
fn vector_connectors_group_icons_but_keep_separate_diagrams() {
    let regions = [
        (0, [0, 0, 100, 80], false),
        (1, [20, 20, 50, 50], true),
        (2, [100, 30, 190, 40], false),
        (3, [190, 0, 290, 80], false),
        (4, [400, 0, 500, 80], false),
    ];
    let groups = visual_regions(&regions);
    assert_eq!(groups.len(), 2);
    assert!(groups.contains(&(0, [0, 0, 290, 80], true)));
    assert!(groups.contains(&(4, [400, 0, 500, 80], false)));
}
