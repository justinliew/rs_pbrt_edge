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
    SpecularReflection, SpecularTransmission,
};
use crate::core::texture::Texture;

// see uber.h

#[derive(Serialize, Deserialize)]
pub struct UberMaterial {
    pub kd: Arc<Texture<Spectrum>>,      // default: 0.25
    pub ks: Arc<Texture<Spectrum>>,      // default: 0.25
    pub kr: Arc<Texture<Spectrum>>,      // default: 0.0
    pub kt: Arc<Texture<Spectrum>>,      // default: 0.0
    pub opacity: Arc<Texture<Spectrum>>, // default: 1.0
    pub roughness: Arc<Texture<Float>>,  // default: 0.1
    pub u_roughness: Option<Arc<Texture<Float>>>,
    pub v_roughness: Option<Arc<Texture<Float>>>,
    pub eta: Arc<Texture<Float>>, // default: 1.5
    pub bump_map: Option<Arc<Texture<Float>>>,
    pub remap_roughness: bool,
}

impl UberMaterial {
    pub fn new(
        kd: Arc<Texture<Spectrum>>,
        ks: Arc<Texture<Spectrum>>,
        kr: Arc<Texture<Spectrum>>,
        kt: Arc<Texture<Spectrum>>,
        roughness: Arc<Texture<Float>>,
        u_roughness: Option<Arc<Texture<Float>>>,
        v_roughness: Option<Arc<Texture<Float>>>,
        opacity: Arc<Texture<Spectrum>>,
        eta: Arc<Texture<Float>>,
        bump_map: Option<Arc<Texture<Float>>>,
        remap_roughness: bool,
    ) -> Self {
        UberMaterial {
            kd,
            ks,
            kr,
            kt,
            opacity,
            roughness,
            u_roughness,
            v_roughness,
            eta,
            bump_map,
            remap_roughness,
        }
    }
    pub fn create(mp: &mut TextureParams) -> Arc<Material> {
        let kd: Arc<Texture<Spectrum>> = mp.get_spectrum_texture("Kd", Spectrum::new(0.25));
        let ks: Arc<Texture<Spectrum>> = mp.get_spectrum_texture("Ks", Spectrum::new(0.25));
        let kr: Arc<Texture<Spectrum>> = mp.get_spectrum_texture("Kr", Spectrum::new(0.0));
        let kt: Arc<Texture<Spectrum>> = mp.get_spectrum_texture("Kt", Spectrum::new(0.0));
        let roughness: Arc<Texture<Float>> = mp.get_float_texture("roughness", 0.1 as Float);
        let u_roughness: Option<Arc<Texture<Float>>> = mp.get_float_texture_or_null("uroughness");
        let v_roughness: Option<Arc<Texture<Float>>> = mp.get_float_texture_or_null("vroughness");
        let opacity: Arc<Texture<Spectrum>> =
            mp.get_spectrum_texture("opacity", Spectrum::new(1.0));
        let bump_map: Option<Arc<Texture<Float>>> = mp.get_float_texture_or_null("bumpmap");
        let remap_roughness: bool = mp.find_bool("remaproughness", true);
        let eta_option: Option<Arc<Texture<Float>>> = mp.get_float_texture_or_null("eta");
        if let Some(ref eta) = eta_option {
            Arc::new(Material::Uber(Box::new(UberMaterial::new(
                kd,
                ks,
                kr,
                kt,
                roughness,
                u_roughness,
                v_roughness,
                opacity,
                eta.clone(),
                bump_map,
                remap_roughness,
            ))))
        } else {
            let eta: Arc<Texture<Float>> = mp.get_float_texture("index", 1.5 as Float);
            Arc::new(Material::Uber(Box::new(UberMaterial::new(
                kd,
                ks,
                kr,
                kt,
                roughness,
                u_roughness,
                v_roughness,
                opacity,
                eta,
                bump_map,
                remap_roughness,
            ))))
        }
    }
    // Material
    pub fn compute_scattering_functions(
        &self,
        si: &mut SurfaceInteraction,
        // arena: &mut Arena,
        mode: TransportMode,
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
        let e: Float = self.eta.evaluate(si);
        let op: Spectrum = self
            .opacity
            .evaluate(si)
            .clamp(0.0 as Float, std::f32::INFINITY as Float);
        let t: Spectrum =
            (Spectrum::new(1.0) - op).clamp(0.0 as Float, std::f32::INFINITY as Float);
        let kd: Spectrum = op
            * self
                .kd
                .evaluate(si)
                .clamp(0.0 as Float, std::f32::INFINITY as Float);
        let ks: Spectrum = op
            * self
                .ks
                .evaluate(si)
                .clamp(0.0 as Float, std::f32::INFINITY as Float);
        let mut u_rough: Float;
        if let Some(ref u_roughness) = self.u_roughness {
            u_rough = u_roughness.evaluate(si);
        } else {
            u_rough = self.roughness.evaluate(si);
        }
        let mut v_rough: Float;
        if let Some(ref v_roughness) = self.v_roughness {
            v_rough = v_roughness.evaluate(si);
        } else {
            v_rough = self.roughness.evaluate(si);
        }
        let kr: Spectrum = op
            * self
                .kr
                .evaluate(si)
                .clamp(0.0 as Float, std::f32::INFINITY as Float);
        let kt: Spectrum = op
            * self
                .kt
                .evaluate(si)
                .clamp(0.0 as Float, std::f32::INFINITY as Float);
        if !t.is_black() {
            si.bsdf = Some(Bsdf::new(si, 1.0));
        } else {
            si.bsdf = Some(Bsdf::new(si, e));
        }
        if let Some(bsdf) = &mut si.bsdf {
            if !t.is_black() {
                if use_scale {
                    bsdf.add(Bxdf::SpecTrans(SpecularTransmission::new(
                        t,
                        1.0,
                        1.0,
                        mode,
                        Some(sc),
                    )));
                } else {
                    bsdf.add(Bxdf::SpecTrans(SpecularTransmission::new(
                        t, 1.0, 1.0, mode, None,
                    )));
                }
            }
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
            if !ks.is_black() {
                let fresnel = Fresnel::Dielectric(FresnelDielectric {
                    eta_i: 1.0,
                    eta_t: e,
                });
                if self.remap_roughness {
                    u_rough = TrowbridgeReitzDistribution::roughness_to_alpha(u_rough);
                    v_rough = TrowbridgeReitzDistribution::roughness_to_alpha(v_rough);
                }
                let distrib = MicrofacetDistribution::TrowbridgeReitz(
                    TrowbridgeReitzDistribution::new(u_rough, v_rough, true),
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
            if !kr.is_black() {
                let fresnel = Fresnel::Dielectric(FresnelDielectric {
                    eta_i: 1.0,
                    eta_t: e,
                });
                if use_scale {
                    bsdf.add(Bxdf::SpecRefl(SpecularReflection::new(
                        kr,
                        fresnel,
                        Some(sc),
                    )));
                } else {
                    bsdf.add(Bxdf::SpecRefl(SpecularReflection::new(kr, fresnel, None)));
                }
            }
            if !kt.is_black() {
                if use_scale {
                    bsdf.add(Bxdf::SpecTrans(SpecularTransmission::new(
                        kt,
                        1.0,
                        e,
                        mode,
                        Some(sc),
                    )));
                } else {
                    bsdf.add(Bxdf::SpecTrans(SpecularTransmission::new(
                        kt, 1.0, e, mode, None,
                    )));
                }
            }
        }
    }
}
