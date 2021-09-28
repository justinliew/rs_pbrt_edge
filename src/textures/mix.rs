// std
use std::ops::{Add, AddAssign, Div, Mul};
use std::sync::Arc;

// pbrt
use crate::core::interaction::SurfaceInteraction;
use crate::core::mipmap::Clampable;
use crate::core::pbrt::Float;
use crate::core::texture::Texture;

#[derive(Serialize, Deserialize)]
pub struct MixTexture<T> {
    pub tex1: Arc<Texture<T>>,
    pub tex2: Arc<Texture<T>>,
    pub amount: Arc<Texture<Float>>,
}

impl<T: Copy> MixTexture<T> {
    pub fn new(tex1: Arc<Texture<T>>, tex2: Arc<Texture<T>>, amount: Arc<Texture<Float>>) -> Self {
        MixTexture { tex1, tex2, amount }
    }
}

impl<T: Copy> MixTexture<T>
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
        let t1: T = self.tex1.evaluate(si);
        let t2: T = self.tex2.evaluate(si);
        let amt: Float = self.amount.evaluate(si);
        t1 * T::from(1.0 as Float - amt) + t2 * T::from(amt)
    }
}
