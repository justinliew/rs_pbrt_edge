// std
use crate::core::pbrt::Float;
use std::ops::{Add, AddAssign, Div, Mul};
use std::sync::Arc;

// pbrt
use crate::core::geometry::{Point2f, Vector2f};
use crate::core::interaction::SurfaceInteraction;
use crate::core::mipmap::Clampable;
use crate::core::texture::{Texture, TextureMapping2D};

// checkerboard.h
#[derive(Serialize, Deserialize)]
pub struct Checkerboard2DTexture<T> {
    pub tex1: Arc<Texture<T>>,
    pub tex2: Arc<Texture<T>>,
    pub mapping: Box<TextureMapping2D>,
    // TODO: const AAMethod aaMethod;
}

impl<T: Copy> Checkerboard2DTexture<T> {
    pub fn new(
        mapping: Box<TextureMapping2D>,
        tex1: Arc<Texture<T>>,
        tex2: Arc<Texture<T>>, // , TODO: aaMethod
    ) -> Self {
        Checkerboard2DTexture {
            tex1,
            tex2,
            mapping,
        }
    }
}

impl<T: Copy> Checkerboard2DTexture<T> {
    //	impl<T: Copy> Checkerboard2DTexture<T> {
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
        let mut dstdx: Vector2f = Vector2f::default();
        let mut dstdy: Vector2f = Vector2f::default();
        let st: Point2f = self.mapping.map(si, &mut dstdx, &mut dstdy);
        // TODO: if (aaMethod == AAMethod::None) {
        if (st.x.floor() as u32 + st.y.floor() as u32) % 2 == 0 {
            self.tex1.evaluate(si)
        } else {
            self.tex2.evaluate(si)
        }
    }
}
