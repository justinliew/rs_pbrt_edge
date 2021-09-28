// std
use std::ops::{Add, AddAssign, Div, Mul};
use std::path::Path;
use std::sync::Arc;
// others
use image::{DynamicImage, ImageResult};
// pbrt
use crate::core::geometry::{Point2f, Point2i, Vector2f};
use crate::core::interaction::SurfaceInteraction;
use crate::core::mipmap::{Clampable, ImageWrap, MipMap};
use crate::core::pbrt::{Float, Spectrum};
use crate::core::texture::TextureMapping2D;

// see imagemap.h
#[derive(Serialize, Deserialize)]
pub struct ImageTexture<T> {
    pub mapping: Box<TextureMapping2D>,
    pub mipmap: Arc<MipMap<T>>,
}

impl<T> ImageTexture<T>
where
    T: std::default::Default
        + num::Zero
        + std::clone::Clone
        + Add<T, Output = T>
        + AddAssign
        + Clampable
        + Copy
        + Div<Float, Output = T>
        + Mul<T, Output = T>
        + Mul<Float, Output = T>,
{
    pub fn new<F: Fn(&Spectrum) -> T>(
        mapping: Box<TextureMapping2D>,
        filename: String,
        do_trilinear: bool,
        max_aniso: Float,
        wrap_mode: ImageWrap,
        scale: Float,
        gamma: bool,
        convert: F,
    ) -> ImageTexture<T> {
        let path = Path::new(&filename);
        let img_result: ImageResult<DynamicImage> = image::open(path);
        if img_result.is_err() {
            panic!("Error reading \"{}\"", filename);
        }
        let buf = img_result.unwrap();
        let rgb = buf.to_rgb8();
        let res = Point2i {
            x: rgb.width() as i32,
            y: rgb.height() as i32,
        };
        let mut texels: Vec<Spectrum> = rgb
            .pixels()
            .map(|p| {
                let r = Float::from(p[0]) / 255.0;
                let g = Float::from(p[1]) / 255.0;
                let b = Float::from(p[2]) / 255.0;
                Spectrum::rgb(r, g, b)
            })
            .collect();
        // flip image in y; texture coordinate space has (0,0) at the
        // lower left corner.
        for y in 0..res.y / 2 {
            for x in 0..res.x {
                let o1 = (y * res.x + x) as usize;
                let o2 = ((res.y - 1 - y) * res.x + x) as usize;
                texels.swap(o1, o2);
            }
        }
        // instead of convertIn(texels[i], &convertedTexels[i], scale, gamma);
        let converted_texels: Vec<T> = texels
            .iter()
            .map(|p| {
                let s = if gamma {
                    p.inverse_gamma_correct() * scale
                } else {
                    *p * scale
                };
                convert(&s)
            })
            .collect();
        // create _MipMap_ from converted texels (see above)
        let mipmap = Arc::new(MipMap::new(
            res,
            &converted_texels[..],
            do_trilinear,
            max_aniso,
            wrap_mode,
        ));
        ImageTexture { mapping, mipmap }
    }
}

impl<T> ImageTexture<T>
where
    T: std::default::Default
        + num::Zero
        + std::clone::Clone
        + Add<T, Output = T>
        + AddAssign
        + Clampable
        + Copy
        + Div<Float, Output = T>
        + Mul<T, Output = T>
        + Mul<Float, Output = T>,
{
    pub fn evaluate(&self, si: &SurfaceInteraction) -> T {
        // Vector2f dstdx, dstdy;
        // Point2f st = mapping->Map(si, &dstdx, &dstdy);
        // Tmemory mem = mipmap->Lookup(st, dstdx, dstdy);
        // Treturn ret;
        // convertOut(mem, &ret);
        // return ret;
        let mut dstdx: Vector2f = Vector2f::default();
        let mut dstdy: Vector2f = Vector2f::default();
        let st: Point2f = self.mapping.map(si, &mut dstdx, &mut dstdy);
        let mem: T = self.mipmap.lookup_pnt_vec_vec(st, &mut dstdx, &mut dstdy);
        let mut ret = mem;
        ret
    }
}

pub fn convert_to_spectrum(from: &Spectrum) -> Spectrum {
    *from
}

pub fn convert_to_float(from: &Spectrum) -> Float {
    from.y()
}
