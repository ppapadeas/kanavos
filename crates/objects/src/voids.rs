//! Openings without a door or window: a plain hole, or one with lined reveals.

use crate::host::Host;
use crate::params::P;
use crate::parts::{Parts, color};
use crate::sym::{Pen, Sym};

/// Main layer of void symbols.
pub(crate) const LAYER: &str = "opening.symbol";
/// Layer of the lintel lines (the wall above the opening).
pub(crate) const ABOVE: &str = "opening.symbol.above";

/// Plan symbol: lining boards at the jambs and the lintel above, dashed along both faces.
pub(crate) fn symbol(id: &str, sym: &mut Sym, host: &Host, p: &P) {
    let w2 = host.w2();
    if id == "void.lined" {
        let lt = p.get("lining").min(host.w / 4.0);
        sym.layer(LAYER);
        sym.rect(-w2, host.ye, -w2 + lt, host.yi, Pen::Frame);
        sym.rect(w2 - lt, host.ye, w2, host.yi, Pen::Frame);
    }
    if p.flag("lintel") {
        sym.layer(ABOVE);
        sym.line((-w2, host.yi), (w2, host.yi), Pen::Above);
        sym.line((-w2, host.ye), (w2, host.ye), Pen::Above);
    }
}

/// 3D parts: lining boards at the jambs, the head and, for raised openings, the sill.
pub(crate) fn parts(id: &str, parts: &mut Parts, host: &Host, p: &P) {
    if id != "void.lined" {
        return;
    }
    let w2 = host.w2();
    let lt = p.get("lining").min(host.w / 4.0);
    let (y0, y1) = (host.ye, host.yi);
    parts.bx([-w2, y0, 0.0], [-w2 + lt, y1, host.h], color::OAK);
    parts.bx([w2 - lt, y0, 0.0], [w2, y1, host.h], color::OAK);
    parts.bx([-w2 + lt, y0, host.h - lt], [w2 - lt, y1, host.h], color::OAK);
    if host.sill > 0.05 {
        parts.bx([-w2 + lt, y0, 0.0], [w2 - lt, y1, lt], color::OAK);
    }
}

/// The outline of a void above the cut plane: both faces, dashed.
pub(crate) fn overhead_symbol(sym: &mut Sym, host: &Host) {
    let w2 = host.w2();
    sym.layer(ABOVE);
    sym.line((-w2, host.yi), (w2, host.yi), Pen::Above);
    sym.line((-w2, host.ye), (w2, host.ye), Pen::Above);
}
