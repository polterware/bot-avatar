pub mod agent_marks;
pub mod avatar;
pub mod canvas;
pub mod color;
pub mod draw;
pub mod engine;
pub mod mesh;
pub mod plastic;
pub mod plastic_cap;
pub mod plastic_fur;
pub mod presets;
pub mod shapes;
pub mod ticker;
pub mod types;
pub mod wear;

pub use avatar::{Avatar, MascotMood, kind_for_agent, state_for_status};
pub use engine::Calm;

#[cfg(test)]
mod bootstrap_tests {
    use crate::canvas::{Canvas2D, PaintStyle, circle_path};
    use crate::shapes::{body_path_for, part_paths_for};
    use crate::types::BotAvatarType;

    #[test]
    fn all_shapes_and_parts_parse() {
        for kind in BotAvatarType::ALL {
            let p = body_path_for(kind);
            let b = p.bounds();
            assert!(
                b.width() > 20.0 && b.height() > 20.0,
                "bad bounds for {kind}"
            );
            if let Some(parts) = part_paths_for(kind) {
                assert!(!parts.is_empty());
            }
        }
    }

    #[test]
    fn radial_gradient_focal_point_order() {
        // Focal point at (70, 50) inside circle centered at (50, 50) radius 40:
        // stop 0 is white, stop 1 is black. Pixel near (70, 50) must be brighter than (30, 50).
        let mut ctx = Canvas2D::new(100, 100);
        let style = PaintStyle::radial_gradient(
            70.0,
            50.0,
            0.0,
            50.0,
            50.0,
            40.0,
            &[(0.0, "#ffffff"), (1.0, "#000000")],
        );
        ctx.set_fill_style(style);
        let c = circle_path(50.0, 50.0, 40.0).unwrap();
        ctx.fill_path(&c);
        let p_focal = ctx.pixmap.pixel(70, 50).unwrap();
        let p_far = ctx.pixmap.pixel(30, 50).unwrap();
        assert!(
            p_focal.red() > p_far.red(),
            "focal={} far={}",
            p_focal.red(),
            p_far.red()
        );
    }

    #[test]
    fn non_uniform_stroke_renders() {
        let mut ctx = Canvas2D::new(100, 100);
        ctx.translate(50.0, 50.0);
        ctx.rotate(0.25);
        ctx.scale(1.0, 0.34);
        ctx.set_stroke_css("#ffffff");
        ctx.set_line_width(4.0);
        let c = circle_path(0.0, 0.0, 30.0).unwrap();
        ctx.stroke_path(&c);
        let non_empty = ctx.pixmap.data().chunks_exact(4).any(|px| px[3] > 0);
        assert!(non_empty);
    }
}
