//! Match Photo round trip: photograph a box with a known camera, draw the
//! lines a person would, and get the same camera back.

use glam::{DMat4, DVec3, DVec4};
use sketchcraft::matchphoto::solve;

#[test]
fn recovers_the_camera_that_took_the_photo() {
    // from four sides of the building, eye level and from above
    for (eye, target, fov) in [
        (DVec3::new(9.0, -12.0, 1.7), DVec3::new(1.0, 2.0, 1.2), 50.0),
        (DVec3::new(-8.0, -10.0, 1.6), DVec3::new(3.0, 1.0, 1.0), 40.0),
        (DVec3::new(14.0, 11.0, 6.0), DVec3::new(2.0, 1.5, 0.5), 60.0),
        (DVec3::new(-10.0, 9.0, 2.0), DVec3::new(2.0, 2.0, 1.5), 35.0),
    ] {
        shoot(eye, target, fov);
    }
}

fn shoot(eye: DVec3, target: DVec3, fov: f64) {
    let (w, h) = (1600.0, 1000.0);
    let view = DMat4::look_at_rh(eye, target, DVec3::Z);
    let proj = DMat4::perspective_rh(fov.to_radians(), w / h, 0.1, 1000.0);
    let shot = |p: DVec3| {
        let q = proj * view * DVec4::new(p.x, p.y, p.z, 1.0);
        let n = q.truncate() / q.w;
        [(n.x + 1.0) / 2.0 * w, (1.0 - n.y) / 2.0 * h]
    };
    let seg = |a: DVec3, b: DVec3| {
        let (p, q) = (shot(a), shot(b));
        [p[0], p[1], q[0], q[1]]
    };
    // a 6 x 4 x 3 m building at the origin: its top and bottom edges
    let red = [seg(DVec3::new(0., 0., 0.), DVec3::new(6., 0., 0.)), seg(DVec3::new(0., 0., 3.), DVec3::new(6., 0., 3.))];
    let green = [seg(DVec3::new(0., 0., 0.), DVec3::new(0., 4., 0.)), seg(DVec3::new(0., 0., 3.), DVec3::new(0., 4., 3.))];
    let origin = shot(DVec3::ZERO);
    let cam = solve(w, h, red, green, origin, eye.length()).unwrap();
    assert!((cam.fov - fov).abs() < 1e-6, "fov {}", cam.fov);
    assert!(cam.eye.distance(eye) < 1e-6, "eye {:?}", cam.eye);
    let dir = (cam.target - cam.eye).normalize();
    assert!(dir.dot((target - eye).normalize()) > 1.0 - 1e-9, "looking the same way");
}

#[test]
fn parallel_lines_are_refused() {
    let r = solve(100.0, 100.0, [[0., 0., 10., 0.], [0., 5., 10., 5.]], [[0., 0., 0., 10.], [5., 0., 3., 10.]], [50., 50.], 10.0);
    assert!(r.is_err());
}
