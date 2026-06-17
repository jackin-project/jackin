use super::{DamageGrid, RowArena};

#[test]
fn shared_row_arena_recycles_rows_between_grids() {
    let arena = RowArena::default();
    {
        let mut grid = DamageGrid::with_row_arena(3, 8, 8, arena.clone());
        grid.process(b"one\ntwo\nthree\nfour\nfive");
    }
    let recycled_after_drop = arena.recycled_rows();
    assert!(
        recycled_after_drop >= 6,
        "primary + alternate rows should return to shared arena on drop"
    );

    let _next_grid = DamageGrid::with_row_arena(3, 8, 8, arena.clone());
    assert!(
        arena.recycled_rows() < recycled_after_drop,
        "new grids should draw rows from the shared arena before allocating"
    );
}
