use super::Column;
use std::collections::VecDeque;

/// Relight a 5x5 halo. The inner 3x3 is insulated from unknown outside
/// terrain by at least sixteen cells, beyond skylight's attenuation radius.
pub(super) fn relight(columns: &mut [Column]) {
    const SIDE: usize = 80;
    const PLANE: usize = SIDE * SIDE;
    const CELLS: usize = PLANE * 256;
    debug_assert_eq!(columns.len(), 25);
    let mut pass = vec![false; CELLS];
    let mut sky = vec![0u8; CELLS];
    let mut queue = VecDeque::new();
    for (ci, column) in columns.iter_mut().enumerate() {
        column.update_heights();
        let (cx, cz) = (ci % 5 * 16, ci / 5 * 16);
        for y in 0..256 {
            for z in 0..16 {
                for x in 0..16 {
                    let local = y * 256 + z * 16 + x;
                    let i = y * PLANE + (cz + z) * SIDE + cx + x;
                    pass[i] = matches!(column.blocks[local], 0 | 20);
                    // Historical heightmap regeneration can leave the top
                    // layer of the highest section below its scan ceiling.
                    // Vanilla seeds full sky above that height even for an
                    // opaque block there; opacity gates propagation instead.
                    if y as i32 >= column.heights[z * 16 + x] {
                        sky[i] = 15;
                        queue.push_back(i);
                    }
                }
            }
        }
    }
    while let Some(i) = queue.pop_front() {
        let value = sky[i].saturating_sub(1);
        if value == 0 {
            continue;
        }
        let (x, z, y) = (i % SIDE, i / SIDE % SIDE, i / PLANE);
        let mut visit = |j: usize| {
            if pass[j] && sky[j] < value {
                sky[j] = value;
                queue.push_back(j);
            }
        };
        if x > 0 {
            visit(i - 1);
        }
        if x + 1 < SIDE {
            visit(i + 1);
        }
        if z > 0 {
            visit(i - SIDE);
        }
        if z + 1 < SIDE {
            visit(i + SIDE);
        }
        if y > 0 {
            visit(i - PLANE);
        }
        if y + 1 < 256 {
            visit(i + PLANE);
        }
    }
    for (ci, column) in columns.iter_mut().enumerate() {
        let (cx, cz) = (ci % 5 * 16, ci / 5 * 16);
        for y in 0..256 {
            for z in 0..16 {
                for x in 0..16 {
                    column.sky[y * 256 + z * 16 + x] = sky[y * PLANE + (cz + z) * SIDE + cx + x];
                }
            }
        }
    }
}
