// std
use std::ops::{Add, AddAssign, Div, Mul};
use std::sync::Arc;

// pbrt
use crate::core::geometry::{Point2f, Vector2f};
use crate::core::interaction::SurfaceInteraction;
use crate::core::mipmap::Clampable;
use crate::core::pbrt::Float;
use crate::core::texture::noise_flt;
use crate::core::texture::{Texture, TextureMapping2D};

// see dots.h

#[derive(Serialize, Deserialize)]
pub struct DotsTexture<T> {
    pub mapping: Box<TextureMapping2D>,
    pub outside_dot: Arc<Texture<T>>,
    pub inside_dot: Arc<Texture<T>>,
}

impl<T: Copy> DotsTexture<T> {
    pub fn new(
        mapping: Box<TextureMapping2D>,
        outside_dot: Arc<Texture<T>>,
        inside_dot: Arc<Texture<T>>,
    ) -> Self {
        DotsTexture {
            mapping,
            outside_dot,
            inside_dot,
        }
    }
}

impl<T: Copy> DotsTexture<T> {
    pub fn evaluate(&self, si: &SurfaceInteraction) -> T
    where
        T: Copy
            + From<Float>
            + Add<Output = T>
            + Mul<Output = T>
            + Mul<Float, Output = T>
            + Div<Float, Output = T>
            + std::default::Default
            + num::Zero
            + std::clone::Clone
            + AddAssign
            + Clampable,
    {
        // compute cell indices for dots
        let mut dpdx: Vector2f = Vector2f::default();
        let mut dpdy: Vector2f = Vector2f::default();
        let st: Point2f = self.mapping.map(si, &mut dpdx, &mut dpdy);
        let s_cell: i32 = (st.x + 0.5 as Float).floor() as i32;
        let t_cell: i32 = (st.y + 0.5 as Float).floor() as i32;
        // return _insideDot_ result if point is inside dot
        if noise_flt(
            s_cell as Float + 0.5 as Float,
            t_cell as Float + 0.5 as Float,
            0.5 as Float, // default
        ) > 0.0 as Float
        {
            let radius: Float = 0.35 as Float;
            let max_shift: Float = 0.5 as Float - radius;
            let s_center: Float = s_cell as Float
                + max_shift
                    * noise_flt(
                        s_cell as Float + 1.5 as Float,
                        t_cell as Float + 2.8 as Float,
                        0.5 as Float, // default
                    );
            let t_center: Float = t_cell as Float
                + max_shift
                    * noise_flt(
                        s_cell as Float + 4.5 as Float,
                        t_cell as Float + 9.8 as Float,
                        0.5 as Float, // default
                    );
            let dst: Vector2f = st
                - Point2f {
                    x: s_center,
                    y: t_center,
                };
            if dst.length_squared() < radius * radius {
                return self.inside_dot.evaluate(si);
            }
        }
        self.outside_dot.evaluate(si)
    }
}
