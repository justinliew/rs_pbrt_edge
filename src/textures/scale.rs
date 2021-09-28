// std
use crate::core::pbrt::Float;
use std::ops::{Add, AddAssign, Div, Mul};
use std::sync::Arc;

// pbrt
use crate::core::interaction::SurfaceInteraction;
use crate::core::mipmap::Clampable;
use crate::core::texture::Texture;

#[derive(Serialize, Deserialize)]
pub struct ScaleTexture<T> {
    pub tex1: Arc<Texture<T>>,
    pub tex2: Arc<Texture<T>>,
}

impl<T: Copy> ScaleTexture<T> {
    pub fn new(tex1: Arc<Texture<T>>, tex2: Arc<Texture<T>>) -> Self {
        ScaleTexture { tex1, tex2 }
    }
}

impl<T: Copy> ScaleTexture<T>
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
    pub fn evaluate(&self, si: &SurfaceInteraction) -> T {
        self.tex1.evaluate(si) * self.tex2.evaluate(si)
    }
}
