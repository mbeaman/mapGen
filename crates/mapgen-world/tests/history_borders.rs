//! The Phase-4 war loop redraws the map: a won war reassigns the loser's
//! frontier cells to the winner (`mearsheimer::transfer_border_cells`), so the
//! rendered borders are *present-day* (post-conquest), not founding borders.
//! That reassignment is unit-tested for panic-safety in the history crate; this
//! pins the end-to-end effect on real worlds — borders demonstrably move, and
//! the total controlled-cell count is conserved (cells change owner, none vanish).

use mapgen_world::{GenerateParams, Pipeline, PipelineStage};

/// Controlling polity per cell just before the History stage (founding borders)
/// and after it (post-conquest borders).
fn pre_post_control(seed: u64) -> (Vec<Option<u32>>, Vec<Option<u32>>) {
    let params = GenerateParams {
        seed,
        cell_count: 4_000,
        ..Default::default()
    };
    let mut pipe = Pipeline::new(params);
    let mut pre: Vec<Option<u32>> = Vec::new();
    loop {
        match pipe.step() {
            // Naming runs after Polities (which sets control) and before History,
            // and doesn't touch control — so this is the founding map.
            Some(PipelineStage::Naming) => pre = pipe.world().society.control.clone(),
            Some(PipelineStage::History) => break,
            Some(_) => {}
            None => break,
        }
    }
    (pre, pipe.world().society.control.clone())
}

#[test]
fn history_moves_borders_and_conserves_territory() {
    let mut total_changed = 0usize;
    for seed in [1u64, 7, 42, 99, 123] {
        let (pre, post) = pre_post_control(seed);
        assert_eq!(pre.len(), post.len());

        let changed = pre
            .iter()
            .zip(&post)
            .filter(|(a, b)| a.is_some() && a != b)
            .count();
        total_changed += changed;

        // Conquest reassigns cells (Some → different Some); it never frees land,
        // so the count of controlled cells is conserved across the stage.
        let pre_ctrl = pre.iter().filter(|c| c.is_some()).count();
        let post_ctrl = post.iter().filter(|c| c.is_some()).count();
        assert_eq!(
            pre_ctrl, post_ctrl,
            "seed {seed}: controlled-cell count must be conserved ({pre_ctrl} → {post_ctrl})"
        );
    }
    // Observed ~90–140 cells per seed move; require a clear non-trivial total so
    // a regression that silently stopped redrawing borders is caught.
    assert!(
        total_changed > 50,
        "history barely moved any borders ({total_changed} cells across 5 seeds) — \
         conquests should redraw territory"
    );
}
