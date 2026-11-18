//! Paper line work geometry (#19, design step 10): intersections of lines and arcs, trim, extend and offset, as pure
//! 2D maths. The commands built on it are tested in `sheet_linework.rs`.

use sketchcraft::draft::linework::*;
use sketchcraft::drafting::{ItemKind, Shape};

fn shape(pts: &[[f64; 2]]) -> Shape {
    Shape { sheet: 1, pts_mm: pts.to_vec(), ..Default::default() }
}
fn line(a: [f64; 2], b: [f64; 2]) -> ItemKind {
    ItemKind::Line(shape(&[a, b]))
}
fn seg(a: [f64; 2], b: [f64; 2]) -> Curve {
    Curve::Seg(a, b)
}
fn near(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6
}
fn chain(k: &ItemKind) -> Chain {
    Chain::of(k).unwrap()
}
/// A half circle over the x axis: from (-10, 0) over (0, -10) (up the sheet) to (10, 0).
fn half() -> ItemKind {
    ItemKind::Arc(shape(&[[-10.0, 0.0], [0.0, -10.0], [10.0, 0.0]]))
}
fn arc_of(k: &ItemKind) -> ArcC {
    match chain(k).curves[0] {
        Curve::Arc(a) => a,
        _ => panic!("not an arc"),
    }
}

// ---- arcs from three points -------------------------------------------------------------------

#[test]
fn an_arc_from_three_points_finds_its_circle_and_direction() {
    let a = arc_of(&half());
    assert!(near(a.c, [0.0, 0.0]) && (a.r - 10.0).abs() < 1e-9);
    assert!(near(a.at(0.0), [-10.0, 0.0]) && near(a.at(1.0), [10.0, 0.0]) && near(a.at(0.5), [0.0, -10.0]));
    assert!((a.length() - 10.0 * std::f64::consts::PI).abs() < 1e-9);
    // the same ends through the other side: it runs the other way round
    let b = arc_of(&ItemKind::Arc(shape(&[[-10.0, 0.0], [0.0, 10.0], [10.0, 0.0]])));
    assert!(a.sweep.signum() != b.sweep.signum());
    assert!(near(b.at(0.5), [0.0, 10.0]));
    // more than half a circle
    let c = arc_of(&ItemKind::Arc(shape(&[[10.0, 0.0], [-10.0, 0.0], [0.0, -10.0]])));
    assert!(c.sweep.abs() > std::f64::consts::PI);
}

#[test]
fn a_degenerate_arc_is_refused_not_drawn() {
    for pts in [vec![[0.0, 0.0], [5.0, 0.0], [10.0, 0.0]], vec![[0.0, 0.0], [0.0, 0.0], [10.0, 0.0]], vec![[0.0, 0.0], [5.0, 5.0], [0.0, 0.0]], vec![[0.0, 0.0], [5.0, 5.0]]] {
        assert!(Chain::of(&ItemKind::Arc(shape(&pts))).is_err(), "{pts:?}");
    }
    assert!(Chain::of(&line([0.0, 0.0], [0.0, 0.0])).is_err());
    assert!(Chain::of(&line([0.0, 0.0], [f64::NAN, 0.0])).is_err());
    assert!(Chain::of(&ItemKind::Rect(shape(&[[0.0, 0.0], [10.0, 0.0]]))).is_err(), "no height");
}

// ---- intersections ----------------------------------------------------------------------------

#[test]
fn line_line_crossings() {
    let h = intersect(&seg([0.0, 0.0], [10.0, 0.0]), &seg([4.0, -5.0], [4.0, 5.0]));
    assert_eq!(h.len(), 1);
    assert!(near(h[0].p, [4.0, 0.0]) && (h[0].ta - 0.4).abs() < 1e-12 && (h[0].tb - 0.5).abs() < 1e-12);
    // parallel, collinear (overlapping or not), and just short of each other: no crossing
    assert!(intersect(&seg([0.0, 0.0], [10.0, 0.0]), &seg([0.0, 1.0], [10.0, 1.0])).is_empty());
    assert!(intersect(&seg([0.0, 0.0], [10.0, 0.0]), &seg([5.0, 0.0], [15.0, 0.0])).is_empty());
    assert!(intersect(&seg([0.0, 0.0], [10.0, 0.0]), &seg([11.0, -5.0], [11.0, 5.0])).is_empty());
    assert!(intersect(&seg([0.0, 0.0], [10.0, 0.0]), &seg([4.0, 0.001], [4.0, 5.0])).is_empty());
    // touching at an end counts
    assert_eq!(intersect(&seg([0.0, 0.0], [10.0, 0.0]), &seg([10.0, 0.0], [10.0, 5.0])).len(), 1);
    // degenerate segments never panic
    assert!(intersect(&seg([1.0, 1.0], [1.0, 1.0]), &seg([0.0, 0.0], [2.0, 2.0])).is_empty());
}

#[test]
fn line_arc_crossings() {
    let a = Curve::Arc(arc_of(&half()));
    // a vertical chord through the arc's top: one hit (the lower half of the circle is not on the arc)
    let h = intersect(&seg([0.0, -20.0], [0.0, 20.0]), &a);
    assert_eq!(h.len(), 1);
    assert!(near(h[0].p, [0.0, -10.0]) && (h[0].tb - 0.5).abs() < 1e-9);
    // a horizontal chord at y = -6: two hits, at x = +-8
    let h = intersect(&seg([-20.0, -6.0], [20.0, -6.0]), &a);
    assert_eq!(h.len(), 2);
    let mut xs: Vec<f64> = h.iter().map(|x| x.p[0]).collect();
    xs.sort_by(f64::total_cmp);
    assert!((xs[0] + 8.0).abs() < 1e-9 && (xs[1] - 8.0).abs() < 1e-9);
    // the same call with the arguments the other way round gives the same points, with the parameters swapped
    let r = intersect(&a, &seg([-20.0, -6.0], [20.0, -6.0]));
    assert_eq!(r.len(), 2);
    assert!(r.iter().all(|x| (x.p[1] + 6.0).abs() < 1e-9 && (x.tb - 0.3).abs() < 1e-9 || (x.tb - 0.7).abs() < 1e-9));
    // tangent at the top: one hit
    assert_eq!(intersect(&seg([-5.0, -10.0], [5.0, -10.0]), &a).len(), 1);
    // below the arc, and a chord that misses the circle: none
    assert!(intersect(&seg([-20.0, 6.0], [20.0, 6.0]), &a).is_empty());
    assert!(intersect(&seg([-20.0, -11.0], [20.0, -11.0]), &a).is_empty());
    // a segment too short to reach the circle
    assert!(intersect(&seg([0.0, 0.0], [0.0, -9.0]), &a).is_empty());
}

#[test]
fn arc_arc_crossings() {
    let a = Curve::Arc(arc_of(&half()));
    // the left half of a circle of radius 10 centred at (10, 0): meets the first at x = 5, y = +-8.66 (only the upper one is on `a`)
    let b = Curve::Arc(arc_of(&ItemKind::Arc(shape(&[[10.0, -10.0], [0.0, 0.0], [10.0, 10.0]]))));
    let h = intersect(&a, &b);
    assert_eq!(h.len(), 1, "{h:?}");
    assert!(near(h[0].p, [5.0, -75f64.sqrt()]));
    // concentric, far apart, and one inside the other: none
    assert!(intersect(&a, &Curve::Arc(arc_of(&ItemKind::Arc(shape(&[[-5.0, 0.0], [0.0, -5.0], [5.0, 0.0]]))))).is_empty());
    assert!(intersect(&a, &Curve::Arc(arc_of(&ItemKind::Arc(shape(&[[100.0, 0.0], [105.0, -5.0], [110.0, 0.0]]))))).is_empty());
    // touching from outside at (10, 0)... that is the end of `a`: one hit
    let t = Curve::Arc(arc_of(&ItemKind::Arc(shape(&[[10.0, 0.0], [15.0, -5.0], [20.0, 0.0]]))));
    assert_eq!(intersect(&a, &t).len(), 1);
}

// ---- trim -------------------------------------------------------------------------------------

/// A 100 mm line along y = 0 with a vertical boundary at x = 30 and another at x = 70.
fn crossed() -> (ItemKind, Vec<Curve>) {
    (line([0.0, 0.0], [100.0, 0.0]), vec![seg([30.0, -10.0], [30.0, 10.0]), seg([70.0, -10.0], [70.0, 10.0])])
}

#[test]
fn trim_the_middle_span_leaves_two_pieces() {
    let (k, b) = crossed();
    let c = chain(&k);
    let (g, d) = c.nearest([50.0, 0.0]);
    assert!(d < 1e-9);
    let t = trim(&c, g, &c.crossings(&b)).unwrap();
    assert_eq!(t.kept.len(), 2);
    assert!(near(t.cut[0], [30.0, 0.0]) && near(t.cut[1], [70.0, 0.0]));
    assert_eq!(run_points(&t.kept[0]).unwrap(), vec![[0.0, 0.0], [30.0, 0.0]]);
    assert_eq!(run_points(&t.kept[1]).unwrap(), vec![[70.0, 0.0], [100.0, 0.0]]);
    // back to saved items: lines
    assert!(matches!(item_of(&k, &t.kept[0]), Some(ItemKind::Line(_))));
}

#[test]
fn trim_an_end_span_leaves_one_piece() {
    let (k, b) = crossed();
    let c = chain(&k);
    let t = trim(&c, c.nearest([10.0, 0.0]).0, &c.crossings(&b)).unwrap();
    assert_eq!(t.kept.len(), 1);
    assert_eq!(run_points(&t.kept[0]).unwrap(), vec![[30.0, 0.0], [100.0, 0.0]]);
    // only one boundary on the far side: trims to the end of the line
    let one = vec![seg([30.0, -10.0], [30.0, 10.0])];
    let t = trim(&c, c.nearest([60.0, 0.0]).0, &c.crossings(&one)).unwrap();
    assert_eq!(t.kept.len(), 1);
    assert_eq!(run_points(&t.kept[0]).unwrap(), vec![[0.0, 0.0], [30.0, 0.0]]);
    assert!(near(t.cut[1], [100.0, 0.0]));
}

#[test]
fn trim_with_nothing_crossing_is_refused() {
    let k = line([0.0, 0.0], [100.0, 0.0]);
    let c = chain(&k);
    let none: Vec<Curve> = vec![];
    assert!(trim(&c, 0.5, &c.crossings(&none)).unwrap_err().to_string().contains("nothing crosses"));
    // parallel and collinear boundaries are not crossings
    let par = vec![seg([0.0, 5.0], [100.0, 5.0]), seg([20.0, 0.0], [60.0, 0.0])];
    assert!(c.crossings(&par).is_empty());
    // a boundary that only meets an end leaves nothing to cut at
    let end = vec![seg([100.0, -10.0], [100.0, 10.0]), seg([0.0, -10.0], [0.0, 10.0])];
    assert!(c.crossings(&end).is_empty());
    assert!(trim(&c, 0.5, &c.crossings(&end)).is_err());
}

#[test]
fn trim_a_polyline_across_its_corners() {
    // an L: (0,0) to (50,0) to (50,50), crossed at (20,0) and at (50,30)
    let k = ItemKind::Polyline(shape(&[[0.0, 0.0], [50.0, 0.0], [50.0, 50.0]]));
    let b = vec![seg([20.0, -5.0], [20.0, 5.0]), seg([45.0, 30.0], [55.0, 30.0])];
    let c = chain(&k);
    let hits = c.crossings(&b);
    assert_eq!(hits.len(), 2);
    // pick near the corner: the span between the crossings goes, round the corner
    let t = trim(&c, c.nearest([50.0, 1.0]).0, &hits).unwrap();
    assert_eq!(t.kept.len(), 2);
    assert_eq!(run_points(&t.kept[0]).unwrap(), vec![[0.0, 0.0], [20.0, 0.0]]);
    let second = run_points(&t.kept[1]).unwrap();
    assert!(near(second[0], [50.0, 30.0]) && near(second[1], [50.0, 50.0]) && second.len() == 2);
    // a boundary through the corner vertex is found once
    let v = vec![seg([40.0, 10.0], [60.0, -10.0])];
    assert_eq!(c.crossings(&v).len(), 1);
}

#[test]
fn trim_a_rect_opens_it_into_a_polyline() {
    let k = ItemKind::Rect(shape(&[[0.0, 0.0], [40.0, 20.0]]));
    let c = chain(&k);
    assert!(c.closed);
    // a vertical line through the whole rect crosses its top and bottom edges
    let b = vec![seg([10.0, -10.0], [10.0, 30.0])];
    let hits = c.crossings(&b);
    assert_eq!(hits.len(), 2);
    // trim the top edge's left part, from x = 10 round the left side to the bottom edge at x = 10
    let t = trim(&c, c.nearest([5.0, 0.0]).0, &hits).unwrap();
    assert_eq!(t.kept.len(), 1);
    let pts = run_points(&t.kept[0]).unwrap();
    assert!(near(pts[0], [10.0, 20.0]) || near(pts[0], [10.0, 0.0]), "{pts:?}");
    assert!(pts.len() >= 4, "round the right side: {pts:?}");
    assert!(matches!(item_of(&k, &t.kept[0]), Some(ItemKind::Polyline(_))));
    // a single crossing can't cut a span out of a loop
    let one = vec![seg([10.0, -10.0], [10.0, 5.0])];
    assert!(trim(&c, 0.5, &c.crossings(&one)).unwrap_err().to_string().contains("two crossings"));
}

#[test]
fn trim_an_arc_against_a_line_and_an_arc() {
    let k = half();
    let c = chain(&k);
    // a vertical line through the top and a horizontal one cutting off both feet
    let b = vec![seg([0.0, -20.0], [0.0, 5.0])];
    let hits = c.crossings(&b);
    assert_eq!(hits.len(), 1);
    let t = trim(&c, c.nearest([-9.0, -4.0]).0, &hits).unwrap();
    assert_eq!(t.kept.len(), 1);
    let Some(ItemKind::Arc(s)) = item_of(&k, &t.kept[0]) else { panic!("an arc stays an arc") };
    assert!(near(s.pts_mm[0], [0.0, -10.0]) && near(s.pts_mm[2], [10.0, 0.0]));
    let a = ArcC::from_three(s.pts_mm[0], s.pts_mm[1], s.pts_mm[2]).unwrap();
    assert!(near(a.c, [0.0, 0.0]) && (a.r - 10.0).abs() < 1e-6, "same circle");
    // trimmed by another arc
    let other = vec![Curve::Arc(arc_of(&ItemKind::Arc(shape(&[[10.0, -10.0], [0.0, 0.0], [10.0, 10.0]]))))];
    let hits = c.crossings(&other);
    assert_eq!(hits.len(), 1);
    let t = trim(&c, c.nearest([8.0, -6.0]).0, &hits).unwrap();
    assert_eq!(t.kept.len(), 1);
}

// ---- extend -----------------------------------------------------------------------------------

#[test]
fn extend_a_line_to_the_next_line_the_end_nearest_the_pick() {
    let c = chain(&line([0.0, 0.0], [40.0, 0.0]));
    let b = vec![seg([70.0, -10.0], [70.0, 10.0]), seg([55.0, -10.0], [55.0, 10.0]), seg([-30.0, -10.0], [-30.0, 10.0])];
    let (e, end) = extend(&c, [38.0, 0.0], &b, 1000.0).unwrap();
    assert!(end && near(e.end(), [55.0, 0.0]) && near(e.start(), [0.0, 0.0]), "the nearest boundary ahead");
    let (e, end) = extend(&c, [2.0, 1.0], &b, 1000.0).unwrap();
    assert!(!end && near(e.start(), [-30.0, 0.0]) && near(e.end(), [40.0, 0.0]));
    // one that ends right on a boundary goes on to the next
    let c2 = chain(&line([0.0, 0.0], [55.0, 0.0]));
    assert!(near(extend(&c2, [54.0, 0.0], &b, 1000.0).unwrap().0.end(), [70.0, 0.0]));
}

#[test]
fn extend_a_polyline_by_its_last_segment() {
    let c = chain(&ItemKind::Polyline(shape(&[[0.0, 0.0], [30.0, 0.0], [30.0, 20.0]])));
    let b = vec![seg([0.0, 50.0], [60.0, 50.0])];
    let (e, end) = extend(&c, [30.0, 19.0], &b, 1000.0).unwrap();
    assert!(end);
    assert_eq!(run_points(&e.curves).unwrap(), vec![[0.0, 0.0], [30.0, 0.0], [30.0, 50.0]]);
}

#[test]
fn extend_a_line_to_an_arc_and_an_arc_to_a_line() {
    // a line from the centre up toward the arc's top: meets it at (0, -10)
    let c = chain(&line([0.0, 0.0], [0.0, -4.0]));
    let arc = vec![Curve::Arc(arc_of(&half()))];
    let (e, _) = extend(&c, [0.0, -3.0], &arc, 1000.0).unwrap();
    assert!(near(e.end(), [0.0, -10.0]), "{:?}", e.end());
    // the half circle's own end carried on round the circle to a line: the arc runs from (-10,0) over the top to
    // (10,0); extending its end goes on below the axis and meets a horizontal line at y = 6
    let a = chain(&half());
    let b = vec![seg([-20.0, 6.0], [20.0, 6.0])];
    let (e, end) = extend(&a, [10.0, 0.0], &b, 1000.0).unwrap();
    assert!(end);
    let Curve::Arc(g) = e.curves[0] else { panic!() };
    assert!((g.r - 10.0).abs() < 1e-9 && near(g.at(1.0), [8.0, 6.0]) && near(g.at(0.0), [-10.0, 0.0]));
    // and the start end goes the other way round, to the same line on the left
    let (e, end) = extend(&a, [-10.0, 0.0], &b, 1000.0).unwrap();
    assert!(!end);
    let Curve::Arc(g) = e.curves[0] else { panic!() };
    assert!(near(g.at(0.0), [-8.0, 6.0]) && near(g.at(1.0), [10.0, 0.0]), "{:?} {:?}", g.at(0.0), g.at(1.0));
    assert!(g.sweep.abs() > std::f64::consts::PI);
}

#[test]
fn extend_refusals() {
    let c = chain(&line([0.0, 0.0], [40.0, 0.0]));
    let none: Vec<Curve> = vec![];
    assert!(extend(&c, [40.0, 0.0], &none, 1000.0).unwrap_err().to_string().contains("nothing to extend to"));
    // parallel, behind, and beyond reach
    let b = vec![seg([0.0, 5.0], [100.0, 5.0]), seg([-20.0, -5.0], [-20.0, 5.0]), seg([5000.0, -5.0], [5000.0, 5.0])];
    assert!(extend(&c, [40.0, 0.0], &b, 1000.0).is_err());
    // a closed shape has no end
    let r = chain(&ItemKind::Rect(shape(&[[0.0, 0.0], [10.0, 10.0]])));
    assert!(extend(&r, [0.0, 0.0], &b, 1000.0).unwrap_err().to_string().contains("closed"));
    // an arc whose circle never meets the boundary
    let a = chain(&half());
    let far = vec![seg([100.0, -5.0], [100.0, 5.0])];
    assert!(extend(&a, [10.0, 0.0], &far, 1000.0).is_err());
}

// ---- offset -----------------------------------------------------------------------------------

#[test]
fn offset_a_line_either_side() {
    let c = chain(&line([0.0, 0.0], [100.0, 0.0]));
    // running east, left is up the sheet (y down, so negative y)
    let l = offset(&c, 5.0, &Side::Left).unwrap();
    assert!(near(l.start(), [0.0, -5.0]) && near(l.end(), [100.0, -5.0]));
    let r = offset(&c, 5.0, &Side::Right).unwrap();
    assert!(near(r.start(), [0.0, 5.0]));
    // by a point
    assert!(near(offset(&c, 5.0, &Side::Toward([50.0, 30.0])).unwrap().start(), [0.0, 5.0]));
    assert!(near(offset(&c, 5.0, &Side::Toward([50.0, -3.0])).unwrap().start(), [0.0, -5.0]));
    assert!(offset(&c, 5.0, &Side::Toward([50.0, 0.0])).is_err(), "on the line itself");
    assert!(offset(&c, 5.0, &Side::Inside).is_err(), "an open shape has no inside");
    for d in [0.0, -1.0, f64::NAN, f64::INFINITY, 1e9] {
        assert!(offset(&c, d, &Side::Left).is_err(), "{d}");
    }
}

#[test]
fn offset_a_polyline_mitres_its_corners() {
    // an L running east then south: (0,0) (50,0) (50,50)
    let c = chain(&ItemKind::Polyline(shape(&[[0.0, 0.0], [50.0, 0.0], [50.0, 50.0]])));
    // the outside of the turn is the left one here (it turns right, the way the sheet looks)
    let o = offset(&c, 5.0, &Side::Left).unwrap();
    assert_eq!(run_points(&o.curves).unwrap(), vec![[0.0, -5.0], [55.0, -5.0], [55.0, 50.0]]);
    let i = offset(&c, 5.0, &Side::Right).unwrap();
    assert_eq!(run_points(&i.curves).unwrap(), vec![[0.0, 5.0], [45.0, 5.0], [45.0, 50.0]]);
    // too much for the shape: the inner corner would pass the far end
    assert!(offset(&c, 60.0, &Side::Right).is_err());
    // a very sharp turn is bevelled, not shot off the page
    let sharp = chain(&ItemKind::Polyline(shape(&[[0.0, 0.0], [100.0, 0.0], [0.0, 1.0]])));
    let s = offset(&sharp, 2.0, &Side::Left).unwrap();
    assert!(run_points(&s.curves).unwrap().iter().all(|p| p[0].abs() < 1000.0));
}

#[test]
fn offset_an_arc_grows_or_shrinks_its_radius() {
    let c = chain(&half());
    // this arc runs from the left foot over the top (a positive sweep in sheet coordinates): left is outside
    let out = offset(&c, 2.0, &Side::Left).unwrap();
    let Curve::Arc(a) = out.curves[0] else { panic!() };
    assert!((a.r - 12.0).abs() < 1e-9 && near(a.c, [0.0, 0.0]));
    let inn = offset(&c, 2.0, &Side::Right).unwrap();
    let Curve::Arc(a) = inn.curves[0] else { panic!() };
    assert!((a.r - 8.0).abs() < 1e-9);
    // by point: inside the circle, and outside it
    let Curve::Arc(a) = offset(&c, 3.0, &Side::Toward([0.0, -5.0])).unwrap().curves[0] else { panic!() };
    assert!((a.r - 7.0).abs() < 1e-9);
    let Curve::Arc(a) = offset(&c, 3.0, &Side::Toward([0.0, -20.0])).unwrap().curves[0] else { panic!() };
    assert!((a.r - 13.0).abs() < 1e-9);
    // an arc running the other way round has the sides the other way round
    let rev = chain(&ItemKind::Arc(shape(&[[10.0, 0.0], [0.0, -10.0], [-10.0, 0.0]])));
    let Curve::Arc(a) = offset(&rev, 2.0, &Side::Left).unwrap().curves[0] else { panic!() };
    assert!((a.r - 8.0).abs() < 1e-9);
    // collapse refused
    assert!(offset(&c, 10.0, &Side::Right).unwrap_err().to_string().contains("collapse"));
    assert!(offset(&c, 11.0, &Side::Right).is_err());
}

#[test]
fn offset_a_rect_in_or_out() {
    let k = ItemKind::Rect(shape(&[[0.0, 0.0], [40.0, 20.0]]));
    let c = chain(&k);
    let inn = offset(&c, 3.0, &Side::Inside).unwrap();
    assert!(is_axis_rect(&inn));
    assert_eq!(rect_of(&inn).unwrap(), [[3.0, 3.0], [37.0, 17.0]]);
    let out = offset(&c, 3.0, &Side::Outside).unwrap();
    assert_eq!(rect_of(&out).unwrap(), [[-3.0, -3.0], [43.0, 23.0]]);
    let by = offset(&c, 3.0, &Side::Toward([20.0, 10.0])).unwrap();
    assert!(near(rect_of(&by).unwrap()[0], [3.0, 3.0]), "a point inside goes inside");
    // the short side is 20: 10 would meet in the middle, more turns it inside out
    assert!(offset(&c, 10.0, &Side::Inside).is_err());
    assert!(offset(&c, 12.0, &Side::Inside).is_err());
    // a rect whose corners were given the other way round offsets the same way
    let flipped = chain(&ItemKind::Rect(shape(&[[40.0, 20.0], [0.0, 0.0]])));
    assert_eq!(rect_of(&offset(&flipped, 3.0, &Side::Inside).unwrap()).unwrap(), [[3.0, 3.0], [37.0, 17.0]]);
    // a closed polyline goes in and out the same way
    let ring = chain(&ItemKind::Polyline(shape(&[[0.0, 0.0], [40.0, 0.0], [40.0, 20.0], [0.0, 20.0], [0.0, 0.0]])));
    assert!(ring.closed);
    let r = offset(&ring, 3.0, &Side::Inside).unwrap();
    assert!(r.closed && r.bbox() == [3.0, 3.0, 37.0, 17.0]);
}

// ---- robustness -------------------------------------------------------------------------------

#[test]
fn random_shapes_never_panic() {
    // a small deterministic generator
    let mut s = 0x9E3779B97F4A7C15u64;
    let mut rnd = move || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        (s >> 11) as f64 / (1u64 << 53) as f64
    };
    let pt = |r: &mut dyn FnMut() -> f64| -> [f64; 2] {
        // sometimes exactly on a grid, so ends meet and lines run along each other
        if r() < 0.5 { [(r() * 10.0).floor() * 10.0, (r() * 10.0).floor() * 10.0] } else { [r() * 100.0, r() * 100.0] }
    };
    for _ in 0..3000 {
        let kind = match (rnd() * 4.0) as u32 {
            0 => line(pt(&mut rnd), pt(&mut rnd)),
            1 => ItemKind::Arc(shape(&[pt(&mut rnd), pt(&mut rnd), pt(&mut rnd)])),
            2 => ItemKind::Rect(shape(&[pt(&mut rnd), pt(&mut rnd)])),
            _ => ItemKind::Polyline(shape(&[pt(&mut rnd), pt(&mut rnd), pt(&mut rnd), pt(&mut rnd)])),
        };
        let Ok(c) = Chain::of(&kind) else { continue };
        let bounds: Vec<Curve> = (0..6)
            .map(|_| if rnd() < 0.7 { seg(pt(&mut rnd), pt(&mut rnd)) } else if let Some(a) = ArcC::from_three(pt(&mut rnd), pt(&mut rnd), pt(&mut rnd)) { Curve::Arc(a) } else { seg(pt(&mut rnd), pt(&mut rnd)) })
            .collect();
        let hits = c.crossings(&bounds);
        let at = pt(&mut rnd);
        let (g, _) = c.nearest(at);
        if let Ok(t) = trim(&c, g, &hits) {
            for run in &t.kept {
                assert!(run.iter().all(|x| x.length().is_finite()));
                assert!(item_of(&kind, run).is_some());
            }
        }
        if let Ok((e, _)) = extend(&c, at, &bounds, 500.0) {
            assert!(e.length().is_finite() && e.length() >= c.length() - 1e-6, "an extension never shortens");
        }
        for side in [Side::Left, Side::Right, Side::Toward(at)] {
            if let Ok(o) = offset(&c, 1.0 + rnd() * 20.0, &side) {
                assert!(o.length().is_finite());
            }
        }
    }
}
