//std
use serde::{Deserialize, Serialize};
use std::sync::Arc;
// pbrt
use crate::core::bssrdf::SeparableBssrdfAdapter;
use crate::core::interaction::SurfaceInteraction;
use crate::core::material::{Material, TransportMode};
use crate::core::microfacet::{
    BeckmannDistribution, MicrofacetDistribution, TrowbridgeReitzDistribution,
};
use crate::core::pbrt::{Float, Spectrum};
use crate::core::reflection::{
    Bxdf, FourierBSDF, Fresnel, FresnelBlend, FresnelConductor, FresnelDielectric, FresnelNoOp,
    FresnelSpecular, LambertianReflection, LambertianTransmission, MicrofacetReflection,
    MicrofacetTransmission, OrenNayar, SpecularReflection, SpecularTransmission,
};
use crate::core::texture::Texture;
use crate::materials::disney::{
    DisneyClearCoat, DisneyDiffuse, DisneyFakeSS, DisneyMicrofacetDistribution, DisneyRetro,
    DisneySheen,
};
use crate::materials::hair::HairBSDF;

// see mixmat.h

/// The mix material takes two other materials and a texture and uses
/// the value returned by the texture to blend between the two
/// materials at the point being shaded.
#[derive(Serialize, Deserialize)]
pub struct MixMaterial {
    pub m1: Arc<Material>,
    pub m2: Arc<Material>,
    pub scale: Arc<Texture<Spectrum>>, // default: 0.5
}

impl MixMaterial {
    pub fn new(m1: Arc<Material>, m2: Arc<Material>, scale: Arc<Texture<Spectrum>>) -> Self {
        MixMaterial { m1, m2, scale }
    }
    // Material
    pub fn compute_scattering_functions(
        &self,
        si: &mut SurfaceInteraction,
        // arena: &mut Arena,
        mode: TransportMode,
        allow_multiple_lobes: bool,
        _material: Option<Arc<Material>>,
        _scale: Option<Spectrum>,
    ) {
        let s1: Spectrum = self
            .scale
            .evaluate(si)
            .clamp(0.0 as Float, std::f32::INFINITY as Float);
        let s2: Spectrum =
            (Spectrum::new(1.0 as Float) - s1).clamp(0.0 as Float, std::f32::INFINITY as Float);
        let mut si2: SurfaceInteraction = SurfaceInteraction::new(
            &si.common.p,
            &si.common.p_error,
            si.uv,
            &si.common.wo,
            &si.dpdu,
            &si.dpdv,
            &si.dndu,
            &si.dndv,
            si.common.time,
            si.shape,
        );
        self.m1
            .compute_scattering_functions(si, mode, allow_multiple_lobes, None, Some(s1));
        self.m2
            .compute_scattering_functions(&mut si2, mode, allow_multiple_lobes, None, Some(s2));
        if let Some(bsdf1) = &mut si.bsdf {
            if let Some(bsdf2) = &si2.bsdf {
                // get Bxdfs from si2 before it gets out of scope
                for bxdf2 in bsdf2.bxdfs.as_slice() {
                    match bxdf2 {
                        Bxdf::Empty(_bxdf) => break,
                        Bxdf::SpecRefl(bxdf) => {
                            let fresnel = match &bxdf.fresnel {
                                Fresnel::Conductor(fresnel) => {
                                    Fresnel::Conductor(FresnelConductor {
                                        eta_i: fresnel.eta_i,
                                        eta_t: fresnel.eta_t,
                                        k: fresnel.k,
                                    })
                                }
                                Fresnel::Dielectric(fresnel) => {
                                    Fresnel::Dielectric(FresnelDielectric {
                                        eta_i: fresnel.eta_i,
                                        eta_t: fresnel.eta_t,
                                    })
                                }
                                _ => Fresnel::NoOp(FresnelNoOp {}),
                            };
                            bsdf1.add(Bxdf::SpecRefl(SpecularReflection::new(
                                bxdf.r,
                                fresnel,
                                bxdf.sc_opt,
                            )))
                        }
                        Bxdf::SpecTrans(bxdf) => {
                            bsdf1.add(Bxdf::SpecTrans(SpecularTransmission::new(
                                bxdf.t,
                                bxdf.eta_a,
                                bxdf.eta_b,
                                bxdf.mode,
                                bxdf.sc_opt,
                            )))
                        }
                        Bxdf::FresnelSpec(bxdf) => {
                            bsdf1.add(Bxdf::FresnelSpec(FresnelSpecular::new(
                                bxdf.r,
                                bxdf.t,
                                bxdf.eta_a,
                                bxdf.eta_b,
                                bxdf.mode,
                                bxdf.sc_opt,
                            )))
                        }
                        Bxdf::LambertianRefl(bxdf) => bsdf1.add(Bxdf::LambertianRefl(
                            LambertianReflection::new(bxdf.r, bxdf.sc_opt),
                        )),
                        Bxdf::LambertianTrans(bxdf) => bsdf1.add(Bxdf::LambertianTrans(
                            LambertianTransmission::new(bxdf.t, bxdf.sc_opt),
                        )),
                        Bxdf::OrenNayarRefl(bxdf) => bsdf1.add(Bxdf::OrenNayarRefl(OrenNayar {
                            r: bxdf.r,
                            a: bxdf.a,
                            b: bxdf.b,
                            sc_opt: bxdf.sc_opt,
                        })),
                        Bxdf::MicrofacetRefl(bxdf) => {
                            let distribution = match &bxdf.distribution {
                                MicrofacetDistribution::Beckmann(distribution) => {
                                    MicrofacetDistribution::Beckmann(BeckmannDistribution {
                                        alpha_x: distribution.alpha_x,
                                        alpha_y: distribution.alpha_y,
                                        sample_visible_area: distribution.sample_visible_area,
                                    })
                                }
                                MicrofacetDistribution::TrowbridgeReitz(distribution) => {
                                    MicrofacetDistribution::TrowbridgeReitz(
                                        TrowbridgeReitzDistribution {
                                            alpha_x: distribution.alpha_x,
                                            alpha_y: distribution.alpha_y,
                                            sample_visible_area: distribution.sample_visible_area,
                                        },
                                    )
                                }
                                MicrofacetDistribution::DisneyMicrofacet(distribution) => {
                                    MicrofacetDistribution::DisneyMicrofacet(
                                        DisneyMicrofacetDistribution::new(
                                            distribution.inner.alpha_x,
                                            distribution.inner.alpha_y,
                                        ),
                                    )
                                }
                            };
                            let fresnel = match &bxdf.fresnel {
                                Fresnel::Conductor(fresnel) => {
                                    Fresnel::Conductor(FresnelConductor {
                                        eta_i: fresnel.eta_i,
                                        eta_t: fresnel.eta_t,
                                        k: fresnel.k,
                                    })
                                }
                                Fresnel::Dielectric(fresnel) => {
                                    Fresnel::Dielectric(FresnelDielectric {
                                        eta_i: fresnel.eta_i,
                                        eta_t: fresnel.eta_t,
                                    })
                                }
                                _ => Fresnel::NoOp(FresnelNoOp {}),
                            };
                            bsdf1.add(Bxdf::MicrofacetRefl(MicrofacetReflection::new(
                                bxdf.r,
                                distribution,
                                fresnel,
                                bxdf.sc_opt,
                            )))
                        }
                        Bxdf::MicrofacetTrans(bxdf) => {
                            let distribution = match &bxdf.distribution {
                                MicrofacetDistribution::Beckmann(distribution) => {
                                    MicrofacetDistribution::Beckmann(BeckmannDistribution {
                                        alpha_x: distribution.alpha_x,
                                        alpha_y: distribution.alpha_y,
                                        sample_visible_area: distribution.sample_visible_area,
                                    })
                                }
                                MicrofacetDistribution::TrowbridgeReitz(distribution) => {
                                    MicrofacetDistribution::TrowbridgeReitz(
                                        TrowbridgeReitzDistribution {
                                            alpha_x: distribution.alpha_x,
                                            alpha_y: distribution.alpha_y,
                                            sample_visible_area: distribution.sample_visible_area,
                                        },
                                    )
                                }
                                MicrofacetDistribution::DisneyMicrofacet(distribution) => {
                                    MicrofacetDistribution::DisneyMicrofacet(
                                        DisneyMicrofacetDistribution::new(
                                            distribution.inner.alpha_x,
                                            distribution.inner.alpha_y,
                                        ),
                                    )
                                }
                            };
                            bsdf1.add(Bxdf::MicrofacetTrans(MicrofacetTransmission::new(
                                bxdf.t,
                                distribution,
                                bxdf.eta_a,
                                bxdf.eta_b,
                                bxdf.mode,
                                bxdf.sc_opt,
                            )))
                        }
                        Bxdf::FresnelBlnd(bxdf) => {
                            let mut distrib: Option<MicrofacetDistribution> = None;
                            if let Some(distribution) = &bxdf.distribution {
                                distrib = match &distribution {
                                    MicrofacetDistribution::Beckmann(distribution) => Some(
                                        MicrofacetDistribution::Beckmann(BeckmannDistribution {
                                            alpha_x: distribution.alpha_x,
                                            alpha_y: distribution.alpha_y,
                                            sample_visible_area: distribution.sample_visible_area,
                                        }),
                                    ),
                                    MicrofacetDistribution::TrowbridgeReitz(distribution) => {
                                        Some(MicrofacetDistribution::TrowbridgeReitz(
                                            TrowbridgeReitzDistribution {
                                                alpha_x: distribution.alpha_x,
                                                alpha_y: distribution.alpha_y,
                                                sample_visible_area: distribution
                                                    .sample_visible_area,
                                            },
                                        ))
                                    }
                                    MicrofacetDistribution::DisneyMicrofacet(distribution) => {
                                        Some(MicrofacetDistribution::DisneyMicrofacet(
                                            DisneyMicrofacetDistribution::new(
                                                distribution.inner.alpha_x,
                                                distribution.inner.alpha_y,
                                            ),
                                        ))
                                    }
                                }
                            }
                            bsdf1.add(Bxdf::FresnelBlnd(FresnelBlend::new(
                                bxdf.rd,
                                bxdf.rs,
                                distrib,
                                bxdf.sc_opt,
                            )))
                        }
                        Bxdf::Fourier(bxdf) => bsdf1.add(Bxdf::Fourier(FourierBSDF::new(
                            bxdf.bsdf_table.clone(),
                            bxdf.mode,
                            bxdf.sc_opt,
                        ))),
                        Bxdf::Bssrdf(bxdf) => bsdf1.add(Bxdf::Bssrdf(SeparableBssrdfAdapter {
                            bssrdf: bxdf.bssrdf.clone(),
                            mode: bxdf.mode,
                            eta2: bxdf.eta2,
                        })),
                        Bxdf::DisDiff(bxdf) => {
                            bsdf1.add(Bxdf::DisDiff(DisneyDiffuse::new(bxdf.r, bxdf.sc_opt)))
                        }
                        Bxdf::DisSS(bxdf) => bsdf1.add(Bxdf::DisSS(DisneyFakeSS::new(
                            bxdf.r,
                            bxdf.roughness,
                            bxdf.sc_opt,
                        ))),
                        Bxdf::DisRetro(bxdf) => bsdf1.add(Bxdf::DisRetro(DisneyRetro::new(
                            bxdf.r,
                            bxdf.roughness,
                            bxdf.sc_opt,
                        ))),
                        Bxdf::DisSheen(bxdf) => {
                            bsdf1.add(Bxdf::DisSheen(DisneySheen::new(bxdf.r, bxdf.sc_opt)))
                        }
                        Bxdf::DisClearCoat(bxdf) => bsdf1.add(Bxdf::DisClearCoat(
                            DisneyClearCoat::new(bxdf.weight, bxdf.gloss, bxdf.sc_opt),
                        )),
                        Bxdf::Hair(bxdf) => bsdf1.add(Bxdf::Hair(HairBSDF {
                            h: bxdf.h,
                            gamma_o: bxdf.gamma_o,
                            eta: bxdf.eta,
                            sigma_a: bxdf.sigma_a,
                            beta_m: bxdf.beta_m,
                            beta_n: bxdf.beta_n,
                            v: bxdf.v,
                            s: bxdf.s,
                            sin_2k_alpha: bxdf.sin_2k_alpha,
                            cos_2k_alpha: bxdf.cos_2k_alpha,
                            sc_opt: bxdf.sc_opt,
                        })),
                    };
                }
            }
        }
    }
}
