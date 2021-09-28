//std
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// pbrt
use crate::core::interaction::SurfaceInteraction;
use crate::core::material::{Material, TransportMode};
use crate::core::microfacet::{MicrofacetDistribution, TrowbridgeReitzDistribution};
use crate::core::paramset::TextureParams;
use crate::core::pbrt::{Float, Spectrum};
use crate::core::reflection::{
    Bsdf, Bxdf, Fresnel, FresnelDielectric, LambertianReflection, MicrofacetReflection,
};
use crate::core::texture::Texture;

// see plastic.h

/// Plastic can be modeled as a mixture of a diffuse and glossy
/// scattering function.
#[derive(Serialize, Deserialize)]
pub struct PlasticMaterial {
    pub kd: Arc<Texture<Spectrum>>,     // default: 0.25
    pub ks: Arc<Texture<Spectrum>>,     // default: 0.25
    pub roughness: Arc<Texture<Float>>, // default: 0.1
    pub bump_map: Option<Arc<Texture<Float>>>,
    pub remap_roughness: bool,
}

impl PlasticMaterial {
    pub fn new(
        kd: Arc<Texture<Spectrum>>,
        ks: Arc<Texture<Spectrum>>,
        roughness: Arc<Texture<Float>>,
        bump_map: Option<Arc<Texture<Float>>>,
        remap_roughness: bool,
    ) -> Self {
        PlasticMaterial {
            kd,
            ks,
            roughness,
            bump_map,
            remap_roughness,
        }
    }
    pub fn create(mp: &mut TextureParams) -> Arc<Material> {
        let kd = mp.get_spectrum_texture("Kd", Spectrum::new(0.25 as Float));
        let ks = mp.get_spectrum_texture("Ks", Spectrum::new(0.25 as Float));
        let roughness = mp.get_float_texture("roughness", 0.1 as Float);
        let bump_map = mp.get_float_texture_or_null("bumpmap");
        let remap_roughness: bool = mp.find_bool("remaproughness", true);
        Arc::new(Material::Plastic(Box::new(PlasticMaterial::new(
            kd,
            ks,
            roughness,
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
        let kd: Spectrum = self
            .kd
            .evaluate(si)
            .clamp(0.0 as Float, std::f32::INFINITY as Float);
        let ks: Spectrum = self
            .ks
            .evaluate(si)
            .clamp(0.0 as Float, std::f32::INFINITY as Float);
        let mut rough: Float = self.roughness.evaluate(si);
        si.bsdf = Some(Bsdf::new(si, 1.0));
        if let Some(bsdf) = &mut si.bsdf {
            // initialize diffuse component of plastic material
            if !kd.is_black() {
                if use_scale {
                    bsdf.add(Bxdf::LambertianRefl(LambertianReflection::new(
                        kd,
                        Some(sc),
                    )));
                } else {
                    bsdf.add(Bxdf::LambertianRefl(LambertianReflection::new(kd, None)));
                }
            }
            // initialize specular component of plastic material
            if !ks.is_black() {
                let fresnel = Fresnel::Dielectric(FresnelDielectric {
                    eta_i: 1.5 as Float,
                    eta_t: 1.0 as Float,
                });
                // create microfacet distribution _distrib_ for plastic material
                if self.remap_roughness {
                    rough = TrowbridgeReitzDistribution::roughness_to_alpha(rough);
                }
                let distrib = MicrofacetDistribution::TrowbridgeReitz(
                    TrowbridgeReitzDistribution::new(rough, rough, true),
                );
                if use_scale {
                    bsdf.add(Bxdf::MicrofacetRefl(MicrofacetReflection::new(
                        ks,
                        distrib,
                        fresnel,
                        Some(sc),
                    )));
                } else {
                    bsdf.add(Bxdf::MicrofacetRefl(MicrofacetReflection::new(
                        ks, distrib, fresnel, None,
                    )));
                }
            }
        }
    }
}
