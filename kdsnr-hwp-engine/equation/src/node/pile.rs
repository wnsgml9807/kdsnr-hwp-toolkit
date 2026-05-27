use super::super::metrics::{mul_div, SOURCE_PILE_MEASURE};
use super::super::types::{EqNode, EqStyle, LayoutBox, LayoutKind, PileAlign, Positioned};
use super::layout as layout_node;

pub(crate) fn layout(grid: &[Vec<EqNode>], align: PileAlign, fs: f64, style: EqStyle) -> LayoutBox {
    // FUN_00025804: a grid of cells. Each column's width is the max cell width in
    // that column, each row's height the max cell height in that row. Column gap =
    // MulDiv(fs,0x14,100)=0.20·fs, row gap = MulDiv(fs,0xf,100)=0.15·fs. Total width
    // = Σcol + (ncols-1)·colgap, height = Σrow + (nrows-1)·rowgap; the box is centred
    // on the axis (+0x58 = -(height/2)).
    let row_gap = mul_div(fs, 15);
    let col_gap = mul_div(fs, 20);
    let nrows = grid.len();
    let ncols = grid.iter().map(|r| r.len()).max().unwrap_or(0);

    let cells: Vec<Vec<LayoutBox>> = grid
        .iter()
        .map(|r| r.iter().map(|c| layout_node(c, fs, style)).collect())
        .collect();

    let mut col_w = vec![0.0_f64; ncols];
    let mut row_h = vec![0.0_f64; nrows];
    for (ri, r) in cells.iter().enumerate() {
        for (ci, cell) in r.iter().enumerate() {
            col_w[ci] = col_w[ci].max(cell.width);
            row_h[ri] = row_h[ri].max(cell.height);
        }
    }

    let mut col_x = vec![0.0_f64; ncols];
    let mut acc = 0.0;
    for (c, w) in col_w.iter().enumerate() {
        col_x[c] = acc;
        acc += w + col_gap;
    }
    let width = if ncols > 0 { acc - col_gap } else { 0.0 };

    let mut row_y = vec![0.0_f64; nrows];
    let mut acc = 0.0;
    for (r, h) in row_h.iter().enumerate() {
        row_y[r] = acc;
        acc += h + row_gap;
    }
    let height = if nrows > 0 { acc - row_gap } else { 0.0 };

    let mut positioned = Vec::new();
    for (ri, r) in cells.into_iter().enumerate() {
        for (ci, cell) in r.into_iter().enumerate() {
            // FUN_000255e8: x by alignment within the column; y centres in the row.
            let x = col_x[ci]
                + match align {
                    PileAlign::Left => 0.0,
                    PileAlign::Center => (col_w[ci] - cell.width) * 0.5,
                    PileAlign::Right => col_w[ci] - cell.width,
                };
            let y = row_y[ri] + (row_h[ri] - cell.height) * 0.5;
            positioned.push(Positioned { x, y, item: cell });
        }
    }

    LayoutBox {
        width,
        height,
        baseline: height * 0.5,
        kind: LayoutKind::Pile {
            rows: positioned,
            source: SOURCE_PILE_MEASURE,
        },
    }
}
