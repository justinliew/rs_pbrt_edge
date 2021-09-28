// pbrt
use crate::core::interaction::SurfaceInteraction;

// see constant.h

#[derive(Serialize, Deserialize)]
pub struct ConstantTexture<T> {
    pub value: T,
}

impl<T: Copy> ConstantTexture<T> {
    pub fn new(value: T) -> Self {
        ConstantTexture { value }
    }
}

impl<T: Copy> ConstantTexture<T> {
    pub fn evaluate(&self, _si: &SurfaceInteraction) -> T {
        self.value
    }
}
