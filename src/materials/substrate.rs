//std
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// pbrt
use crate::core::interaction::SurfaceInteraction;
use crate::core::material::{Material, TransportMode};
use crate::core::microfacet::{MicrofacetDistribution, TrowbridgeReitzDistribution};
use crate::core::paramset::TextureParams;
use crate::core::pbrt::{Float, Spectrum};
use crate::core::reflection::{Bsdf, Bxdf, FresnelBlend};
use crate::core::texture::Texture;

// see substrate.h

#[derive(Serialize, Deserialize)]
pub struct SubstrateMaterial {
    pub kd: Arc<Texture<Spectrum>>, // default: 0.5
    pub ks: Arc<Texture<Spectrum>>, // default: 0.5
    pub nu: Arc<Texture<Float>>,    // default: 0.1
    pub nv: Arc<Texture<Float>>,    // default: 0.1
    pub bump_map: Option<Arc<Texture<Float>>>,
    pub remap_roughness: bool,
}

impl SubstrateMaterial {
    pub fn new(
        kd: Arc<Texture<Spectrum>>,
        ks: Arc<Texture<Spectrum>>,
        nu: Arc<Texture<Float>>,
        nv: Arc<Texture<Float>>,
        bump_map: Option<Arc<Texture<Float>>>,
        remap_roughness: bool,
    ) -> Self {
        SubstrateMaterial {
            kd,
            ks,
            nu,
            nv,
            bump_map,
            remap_roughness,
        }
    }
    pub fn create(mp: &mut TextureParams) -> Arc<Material> {
        let kd: Arc<Texture<Spectrum>> = mp.get_spectrum_texture("Kd", Spectrum::new(0.5));
        let ks: Arc<Texture<Spectrum>> = mp.get_spectrum_texture("Ks", Spectrum::new(0.5));
        let uroughness: Arc<Texture<Float>> = mp.get_float_texture("uroughness", 0.1);
        let vroughness: Arc<Texture<Float>> = mp.get_float_texture("vroughness", 0.1);
        let bump_map = mp.get_float_texture_or_null("bumpmap");
        let remap_roughness: bool = mp.find_bool("remaproughness", true);
        Arc::new(Material::Substrate(Box::new(SubstrateMaterial::new(
            kd,
            ks,
            uroughness,
            vroughness,
            bump_map,
            remap_roughness,
        ))))
    }
    // Material
    pub fn compute_scattering_functions(
        &self,
        si: &mut SurfaceInteraction,
        // arena: &mut Arena,
        _mode: TransportMode,
        _allow_multiple_lobes: bool,
        _material: Option<Arc<Material>>,
        scale_opt: Option<Spectrum>,
    ) {
        let mut use_scale: bool = false;
        let mut sc: Spectrum = Spectrum::default();
        if let Some(scale) = scale_opt {
            use_scale = true;
            sc = scale;
        }
        if let Some(ref bump) = self.bump_map {
            Material::bump(bump, si);
        }
        let d: Spectrum = self
            .kd
            .evaluate(si)
            .clamp(0.0 as Float, std::f32::INFINITY as Float);
        let s: Spectrum = self
            .ks
            .evaluate(si)
            .clamp(0.0 as Float, std::f32::INFINITY as Float);
        let mut roughu: Float = self.nu.evaluate(si);
        let mut roughv: Float = self.nv.evaluate(si);
        si.bsdf = Some(Bsdf::new(si, 1.0));
        if let Some(bsdf) = &mut si.bsdf {
            if !d.is_black() || !s.is_black() {
                if self.remap_roughness {
                    roughu = TrowbridgeReitzDistribution::roughness_to_alpha(roughu);
                    roughv = TrowbridgeReitzDistribution::roughness_to_alpha(roughv);
                }
                let distrib: Option<MicrofacetDistribution> =
                    Some(MicrofacetDistribution::TrowbridgeReitz(
                        TrowbridgeReitzDistribution::new(roughu, roughv, true),
                    ));
                if use_scale {
                    bsdf.add(Bxdf::FresnelBlnd(FresnelBlend::new(
                        d,
                        s,
                        distrib,
                        Some(sc),
                    )));
                } else {
                    bsdf.add(Bxdf::FresnelBlnd(FresnelBlend::new(d, s, distrib, None)));
                }
            }
        }
    }
}
