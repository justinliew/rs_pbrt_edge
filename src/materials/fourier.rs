//std
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// pbrt
use crate::core::api::BsdfState;
use crate::core::interaction::SurfaceInteraction;
use crate::core::material::{Material, TransportMode};
use crate::core::paramset::TextureParams;
use crate::core::pbrt::{Float, Spectrum};
use crate::core::reflection::{Bsdf, Bxdf, FourierBSDF, FourierBSDFTable};
use crate::core::texture::Texture;

// see fourier.h

#[derive(Serialize, Deserialize)]
pub struct FourierMaterial {
    pub bsdf_table: Arc<FourierBSDFTable>,
    pub bump_map: Option<Arc<Texture<Float>>>,
}

impl FourierMaterial {
    pub fn new(bsdf_table: Arc<FourierBSDFTable>, bump_map: Option<Arc<Texture<Float>>>) -> Self {
        FourierMaterial {
            bump_map,
            bsdf_table,
        }
    }
    pub fn create(mp: &mut TextureParams, bsdf_state: &mut BsdfState) -> Arc<Material> {
        let bump_map: Option<Arc<Texture<Float>>> = mp.get_float_texture_or_null("bumpmap");
        let bsdffile: String = mp.find_filename("bsdffile", String::new());
        if let Some(bsdf_table) = bsdf_state.loaded_bsdfs.get(&bsdffile) {
            // use the BSDF table found
            Arc::new(Material::Fourier(Box::new(FourierMaterial::new(
                bsdf_table.clone(),
                bump_map,
            ))))
        } else {
            // read BSDF table from file
            let mut bsdf_table: FourierBSDFTable = FourierBSDFTable::default();
            println!(
                "reading {:?} returns {}",
                bsdffile,
                bsdf_table.read(&bsdffile)
            );
            let bsdf_table_arc: Arc<FourierBSDFTable> = Arc::new(bsdf_table);
            // TODO: bsdf_state.loaded_bsdfs.insert(bsdffile.clone(), bsdf_table_arc.clone());
            Arc::new(Material::Fourier(Box::new(FourierMaterial::new(
                bsdf_table_arc,
                bump_map,
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
        si.bsdf = Some(Bsdf::new(si, 1.0));
        if let Some(bsdf) = &mut si.bsdf {
            if use_scale {
                bsdf.add(Bxdf::Fourier(FourierBSDF::new(
                    self.bsdf_table.clone(),
                    mode,
                    Some(sc),
                )));
            } else {
                bsdf.add(Bxdf::Fourier(FourierBSDF::new(
                    self.bsdf_table.clone(),
                    mode,
                    None,
                )));
            }
        }
    }
}
