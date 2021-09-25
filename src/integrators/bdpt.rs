// std
use std::cell::Cell;
use std::f32::consts::PI;
use std::sync::Arc;
// pbrt
use crate::blockqueue::BlockQueue;
use crate::core::camera::{Camera, CameraSample};
use crate::core::film::{Film, FilmTile};
use crate::core::geometry::{
    nrm_abs_dot_vec3f, pnt2_inside_exclusivei, pnt3_offset_ray_origin, vec3_abs_dot_nrmf,
    vec3_dot_nrmf,
};
use crate::core::geometry::{
    Bounds2i, Bounds3f, Normal3f, Point2f, Point2i, Point3f, Ray, Vector2i, Vector3f,
};
use crate::core::interaction::{
    Interaction, InteractionCommon, MediumInteraction, SurfaceInteraction,
};
use crate::core::light::is_delta_light;
use crate::core::light::{Light, LightFlags, VisibilityTester};
use crate::core::lightdistrib::create_light_sample_distribution;
use crate::core::material::TransportMode;
use crate::core::medium::{HenyeyGreenstein, Medium, MediumInterface};
use crate::core::pbrt::{Float, Spectrum};
use crate::core::reflection::Bsdf;
use crate::core::reflection::BxdfType;
use crate::core::sampler::Sampler;
use crate::core::sampling::Distribution1D;
use crate::core::scene::Scene;

#[cfg(not(feature = "ecp"))]
#[cfg(not(test))]
use wasm_bindgen::prelude::*;

#[cfg(not(feature = "ecp"))]
#[cfg(not(test))]
use std::os::raw::{c_char, c_int, c_uint};

#[cfg(not(feature = "ecp"))]
#[cfg(not(test))]
#[wasm_bindgen(raw_module = "./request.js")]
extern "C" {
    pub fn http_request(x: c_uint, u: c_uint, size: c_int, data: String);
}

// see bdpt.h

#[derive(Default)]
pub struct EndpointInteraction<'a> {
    // Interaction Public Data
    pub common: InteractionCommon,
    // EndpointInteraction Public Data
    pub camera: Option<&'a Arc<Camera>>,
    pub light: Option<&'a Arc<Light>>,
}

impl<'a> EndpointInteraction<'a> {
    pub fn new(p: &Point3f, time: Float) -> Self {
        let mut common: InteractionCommon = InteractionCommon::default();
        common.p = *p;
        common.time = time;
        EndpointInteraction {
            common,
            ..Default::default()
        }
    }
    pub fn new_interaction_from_camera(it: &InteractionCommon, camera: &'a Arc<Camera>) -> Self {
        let mut ei: EndpointInteraction = EndpointInteraction::new(&it.p, it.time);
        {
            ei.common.p_error = it.p_error;
            ei.common.wo = it.wo;
            ei.common.n = it.n;
        }
        ei.camera = Some(camera);
        ei
    }
    pub fn new_camera(camera: &'a Arc<Camera>, ray: &Ray) -> Self {
        let mut common: InteractionCommon = InteractionCommon::default();
        common.p = ray.o;
        common.time = ray.time;
        let mut ei: EndpointInteraction = EndpointInteraction {
            common,
            camera: Some(camera),
            ..Default::default()
        };
        if let Some(ref medium_arc) = ray.medium {
            let inside: Option<Arc<Medium>> = Some(medium_arc.clone());
            let outside: Option<Arc<Medium>> = Some(medium_arc.clone());
            {
                ei.common.medium_interface = Some(Arc::new(MediumInterface::new(inside, outside)));
            }
        }
        ei
    }
    pub fn new_light(light: &'a Arc<Light>, ray: &Ray, nl: &Normal3f) -> Self {
        let mut common: InteractionCommon = InteractionCommon::default();
        common.p = ray.o;
        common.time = ray.time;
        let mut ei: EndpointInteraction = EndpointInteraction {
            common,
            light: Some(light),
            ..Default::default()
        };
        if let Some(ref medium_arc) = ray.medium {
            let inside: Option<Arc<Medium>> = Some(medium_arc.clone());
            let outside: Option<Arc<Medium>> = Some(medium_arc.clone());
            {
                ei.common.medium_interface = Some(Arc::new(MediumInterface::new(inside, outside)));
            }
        }
        {
            ei.common.n = *nl;
        }
        ei
    }
    pub fn new_interaction_from_light(it: &InteractionCommon, light: &'a Arc<Light>) -> Self {
        let mut ei: EndpointInteraction = EndpointInteraction::default();
        {
            ei.common.p = it.p;
            ei.common.time = it.time;
            ei.common.p_error = it.p_error;
            ei.common.wo = it.wo;
            ei.common.n = it.n;
            if let Some(ref medium_interface_arc) = it.medium_interface {
                ei.common.medium_interface = Some(medium_interface_arc.clone());
            }
        }
        ei.light = Some(light);
        ei
    }
    pub fn new_ray(ray: &Ray) -> Self {
        let mut common: InteractionCommon = InteractionCommon::default();
        common.p = ray.position(1.0 as Float);
        common.time = ray.time;
        let mut ei: EndpointInteraction = EndpointInteraction {
            common,
            ..Default::default()
        };
        {
            ei.common.n = Normal3f::from(-ray.d);
            if let Some(ref medium_arc) = ray.medium {
                let inside: Option<Arc<Medium>> = Some(medium_arc.clone());
                let outside: Option<Arc<Medium>> = Some(medium_arc.clone());
                ei.common.medium_interface = Some(Arc::new(MediumInterface::new(inside, outside)));
            }
        }
        ei
    }
    pub fn get_medium(&self, w: &Vector3f) -> Option<Arc<Medium>> {
        if vec3_dot_nrmf(w, &self.common.n) > 0.0 as Float {
            if let Some(ref medium_interface) = self.common.medium_interface {
                medium_interface.outside.clone()
            } else {
                None
            }
        } else if let Some(ref medium_interface) = self.common.medium_interface {
            medium_interface.inside.clone()
        } else {
            None
        }
    }
}

impl<'a> Interaction for EndpointInteraction<'a> {
    fn is_surface_interaction(&self) -> bool {
        self.common.n != Normal3f::default()
    }
    fn is_medium_interaction(&self) -> bool {
        !self.is_surface_interaction()
    }
    fn spawn_ray(&self, d: &Vector3f) -> Ray {
        let o: Point3f =
            pnt3_offset_ray_origin(&self.common.p, &self.common.p_error, &self.common.n, d);
        Ray {
            o,
            d: *d,
            t_max: Cell::new(std::f32::INFINITY),
            time: self.common.time,
            differential: None,
            medium: self.get_medium(d),
        }
    }
    fn get_common(&self) -> &InteractionCommon {
        &self.common
    }
    fn get_p(&self) -> &Point3f {
        &self.common.p
    }
    fn get_time(&self) -> Float {
        self.common.time
    }
    fn get_p_error(&self) -> &Vector3f {
        &self.common.p_error
    }
    fn get_wo(&self) -> &Vector3f {
        &self.common.wo
    }
    fn get_n(&self) -> &Normal3f {
        &self.common.n
    }
    fn get_medium_interface(&self) -> Option<Arc<MediumInterface>> {
        if let Some(ref medium_interface) = self.common.medium_interface {
            Some(medium_interface.clone())
        } else {
            None
        }
    }
    fn get_bsdf(&self) -> Option<&Bsdf> {
        None
    }
    fn get_shading_n(&self) -> Option<&Normal3f> {
        None
    }
    fn get_phase(&self) -> Option<Arc<HenyeyGreenstein>> {
        None
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum VertexType {
    Camera,
    Light,
    Surface,
    Medium,
}

pub struct Vertex<'a> {
    vertex_type: VertexType,
    beta: Spectrum,
    ei: Option<EndpointInteraction<'a>>,
    mi: Option<MediumInteraction>,
    si: Option<SurfaceInteraction<'a>>,
    delta: bool,
    pdf_fwd: Float,
    pdf_rev: Float,
}

impl<'a> Vertex<'a> {
    pub fn new(vertex_type: VertexType, ei: EndpointInteraction<'a>, beta: &Spectrum) -> Self {
        Vertex {
            vertex_type,
            beta: *beta,
            ei: Some(ei),
            mi: None,
            si: None,
            delta: false,
            pdf_fwd: 0.0 as Float,
            pdf_rev: 0.0 as Float,
        }
    }
    pub fn create_camera_from_ray(
        camera: &'a Arc<Camera>,
        ray: &Ray,
        beta: &Spectrum,
    ) -> Vertex<'a> {
        Vertex::new(
            VertexType::Camera,
            EndpointInteraction::new_camera(camera, ray),
            beta,
        )
    }
    pub fn create_camera_from_interaction(
        camera: &'a Arc<Camera>,
        it: &InteractionCommon,
        beta: &Spectrum,
    ) -> Vertex<'a> {
        Vertex::new(
            VertexType::Camera,
            EndpointInteraction::new_interaction_from_camera(it, camera),
            beta,
        )
    }
    pub fn create_surface_interaction(
        si: SurfaceInteraction<'a>,
        beta: &Spectrum,
        pdf: Float,
        prev: &Vertex,
    ) -> Vertex<'a> {
        let mut v: Vertex = Vertex {
            vertex_type: VertexType::Surface,
            beta: *beta,
            ei: None,
            mi: None,
            si: Some(si),
            delta: false,
            pdf_fwd: 0.0 as Float,
            pdf_rev: 0.0 as Float,
        };
        v.pdf_fwd = prev.convert_density(pdf, &v);
        v
    }
    pub fn create_medium_interaction(
        mi: MediumInteraction,
        beta: &Spectrum,
        pdf: Float,
        prev: &Vertex,
    ) -> Vertex<'a> {
        let mut v: Vertex = Vertex {
            vertex_type: VertexType::Medium,
            beta: *beta,
            ei: None,
            mi: Some(mi),
            si: None,
            delta: false,
            pdf_fwd: 0.0 as Float,
            pdf_rev: 0.0 as Float,
        };
        v.pdf_fwd = prev.convert_density(pdf, &v);
        v
    }
    pub fn create_light_interaction(
        ei: EndpointInteraction<'a>,
        beta: &Spectrum,
        pdf: Float,
    ) -> Vertex<'a> {
        let mut v: Vertex = Vertex::new(VertexType::Light, ei, beta);
        v.pdf_fwd = pdf;
        v
    }
    pub fn create_light(
        light: &'a Arc<Light>,
        ray: &Ray,
        nl: &Normal3f,
        le: &Spectrum,
        pdf: Float,
    ) -> Vertex<'a> {
        let mut v = Vertex::new(
            VertexType::Light,
            EndpointInteraction::new_light(light, ray, nl),
            le,
        );
        v.pdf_fwd = pdf;
        v
    }
    pub fn p(&self) -> Point3f {
        match self.vertex_type {
            VertexType::Medium => {
                if let Some(ref mi) = self.mi {
                    mi.common.p
                } else {
                    Point3f::default()
                }
            }
            VertexType::Surface => {
                if let Some(ref si) = self.si {
                    si.common.p
                } else {
                    Point3f::default()
                }
            }
            _ => {
                if let Some(ref ei) = self.ei {
                    ei.common.p
                } else {
                    Point3f::default()
                }
            }
        }
    }
    pub fn time(&self) -> Float {
        match self.vertex_type {
            VertexType::Medium => {
                if let Some(ref mi) = self.mi {
                    mi.common.time
                } else {
                    Float::default()
                }
            }
            VertexType::Surface => {
                if let Some(ref si) = self.si {
                    si.common.time
                } else {
                    Float::default()
                }
            }
            _ => {
                if let Some(ref ei) = self.ei {
                    ei.common.time
                } else {
                    Float::default()
                }
            }
        }
    }
    pub fn ng(&self) -> Normal3f {
        match self.vertex_type {
            VertexType::Medium => {
                if let Some(ref mi) = self.mi {
                    mi.common.n
                } else {
                    Normal3f::default()
                }
            }
            VertexType::Surface => {
                if let Some(ref si) = self.si {
                    si.common.n
                } else {
                    Normal3f::default()
                }
            }
            _ => {
                if let Some(ref ei) = self.ei {
                    ei.common.n
                } else {
                    Normal3f::default()
                }
            }
        }
    }
    pub fn ns(&self) -> Normal3f {
        match self.vertex_type {
            VertexType::Medium => {
                if let Some(ref mi) = self.mi {
                    mi.common.n
                } else {
                    Normal3f::default()
                }
            }
            VertexType::Surface => {
                if let Some(ref si) = self.si {
                    si.shading.n
                } else {
                    Normal3f::default()
                }
            }
            VertexType::Light => {
                if let Some(ref ei) = self.ei {
                    ei.common.n
                } else {
                    Normal3f::default()
                }
            }
            _ => Normal3f::default(),
        }
    }
    pub fn is_on_surface(&self) -> bool {
        self.ng() != Normal3f::default()
    }
    pub fn f(&self, next: &Vertex, mode: TransportMode) -> Spectrum {
        let mut wi: Vector3f = next.p() - self.p();
        if wi.length_squared() == 0.0 as Float {
            return Spectrum::default();
        }
        wi = wi.normalize();
        match self.vertex_type {
            VertexType::Surface => {
                if let Some(ref si) = self.si {
                    if let Some(ref bsdf) = si.bsdf {
                        let bsdf_flags: u8 = BxdfType::BsdfAll as u8;
                        bsdf.f(&si.common.wo, &wi, bsdf_flags)
                            * correct_shading_normal(si, &si.common.wo, &wi, mode)
                    } else {
                        Spectrum::default()
                    }
                } else {
                    Spectrum::default()
                }
            }
            VertexType::Medium => {
                if let Some(ref mi) = self.mi {
                    if let Some(phase) = mi.get_phase() {
                        Spectrum::new(phase.p(&mi.common.wo, &wi))
                    } else {
                        Spectrum::default()
                    }
                } else {
                    Spectrum::default()
                }
            }
            _ => Spectrum::default(),
        }
    }
    pub fn is_connectible(&self) -> bool {
        match self.vertex_type {
            VertexType::Medium => true,
            VertexType::Light => {
                if let Some(ref ei) = self.ei {
                    if let Some(ref light) = ei.light {
                        let check: u8 = light.get_flags() & LightFlags::DeltaDirection as u8;
                        check == 0 as u8
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            VertexType::Camera => true,
            VertexType::Surface => {
                if let Some(ref si) = self.si {
                    if let Some(ref bsdf) = si.bsdf {
                        let bsdf_flags: u8 = BxdfType::BsdfDiffuse as u8
                            | BxdfType::BsdfGlossy as u8
                            | BxdfType::BsdfReflection as u8
                            | BxdfType::BsdfTransmission as u8;
                        bsdf.num_components(bsdf_flags) > 0
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
        }
    }
    pub fn is_light(&self) -> bool {
        if self.vertex_type == VertexType::Light {
            return true;
        } else if self.vertex_type == VertexType::Surface {
            if let Some(ref si) = self.si {
                if let Some(primitive_raw) = si.primitive {
                    let primitive = unsafe { &*primitive_raw };
                    if let Some(_area_light) = primitive.get_area_light() {
                        return true;
                    }
                }
            }
        }
        false
    }
    pub fn is_delta_light(&self) -> bool {
        if self.vertex_type != VertexType::Light {
            return false;
        } else if let Some(ref ei) = self.ei {
            if let Some(ref light) = ei.light {
                let check: u8 = light.get_flags();
                return is_delta_light(check);
            }
        }
        false
    }
    pub fn is_infinite_light(&self) -> bool {
        if self.vertex_type != VertexType::Light {
            return false;
        } else if let Some(ref ei) = self.ei {
            if let Some(ref light) = ei.light {
                let check: u8 = light.get_flags() & LightFlags::Infinite as u8;
                if check == LightFlags::Infinite as u8 {
                    return true;
                }
                let check: u8 = light.get_flags() & LightFlags::DeltaDirection as u8;
                if check == LightFlags::DeltaDirection as u8 {
                    return true;
                }
            } else {
                // !ei.light
                return true;
            }
        }
        false
    }
    pub fn le(&self, scene: &Scene, v: &Vertex) -> Spectrum {
        if !self.is_light() {
            return Spectrum::default();
        }
        let mut w: Vector3f = v.p() - self.p();
        if w.length_squared() == 0.0 as Float {
            return Spectrum::default();
        }
        w = w.normalize();
        if self.is_infinite_light() {
            // return emitted radiance for infinite light sources
            let mut le: Spectrum = Spectrum::default();
            for light in &scene.infinite_lights {
                let mut ray: Ray = Ray {
                    o: self.p(),
                    d: -w,
                    t_max: Cell::new(Float::default()),
                    time: Float::default(),
                    differential: None,
                    medium: None,
                };
                le += light.le(&mut ray);
            }
            return le;
        } else if let Some(ref si) = self.si {
            if let Some(primitive_raw) = si.primitive {
                let primitive = unsafe { &*primitive_raw };
                if let Some(light) = primitive.get_area_light() {
                    let mut iref: InteractionCommon = InteractionCommon::default();
                    iref.p = si.common.p;
                    iref.time = si.common.time;
                    iref.p_error = si.common.p_error;
                    iref.wo = si.common.wo;
                    iref.n = si.common.n;
                    return light.l(&iref, &w);
                }
            }
        }
        Spectrum::default()
    }
    pub fn convert_density(&self, pdf: Float, next: &Vertex) -> Float {
        // return solid angle density if _next_ is an infinite area light
        if next.is_infinite_light() {
            return pdf;
        }
        let w: Vector3f = next.p() - self.p();
        if w.length_squared() == 0.0 as Float {
            return 0.0 as Float;
        }
        let inv_dist_2: Float = 1.0 as Float / w.length_squared();
        let mut pdf: Float = pdf; // shadow
        if next.is_on_surface() {
            pdf *= nrm_abs_dot_vec3f(&next.ng(), &(w * inv_dist_2.sqrt()));
        }
        pdf * inv_dist_2
    }
    pub fn pdf(&self, scene: &Scene, prev: Option<&Vertex>, next: &Vertex) -> Float {
        // if (type == VertexType::Light) return PdfLight(scene, next);
        if self.vertex_type == VertexType::Light {
            return self.pdf_light(scene, next);
        }
        // compute directions to preceding and next vertex
        let mut wn: Vector3f = next.p() - self.p();
        if wn.length_squared() == 0.0 as Float {
            return 0.0 as Float;
        }
        wn = wn.normalize();
        let mut wp: Vector3f = Vector3f::default();
        if let Some(prev) = prev {
            wp = prev.p() - self.p();
            if wp.length_squared() == 0.0 as Float {
                return 0.0 as Float;
            }
            wp = wp.normalize();
        } else {
            assert!(
                self.vertex_type == VertexType::Camera,
                "VertexType::Camera expected, VertexType::{:?} found",
                self.vertex_type
            );
        }
        // compute directional density depending on the vertex types
        let mut pdf_flt: Float = 0.0;
        let mut _unused: Float = 0.0;
        if self.vertex_type == VertexType::Camera {
            if let Some(ref ei) = self.ei {
                if let Some(ref camera) = ei.camera {
                    let (_unused, pdf_flt_local) = camera.pdf_we(&ei.spawn_ray(&wn));
                    pdf_flt = pdf_flt_local;
                }
            }
        } else if self.vertex_type == VertexType::Surface {
            if let Some(ref si) = self.si {
                if let Some(ref bsdf) = si.bsdf {
                    let bsdf_flags: u8 = BxdfType::BsdfAll as u8;
                    pdf_flt = bsdf.pdf(&wp, &wn, bsdf_flags);
                }
            }
        } else if self.vertex_type == VertexType::Medium {
            if let Some(ref mi) = self.mi {
                if let Some(ref phase) = mi.phase {
                    pdf_flt = phase.p(&wp, &wn);
                }
            }
        }
        // return probability per unit area at vertex _next_
        self.convert_density(pdf_flt, next)
    }
    pub fn pdf_light(&self, scene: &Scene, v: &Vertex) -> Float {
        let mut w: Vector3f = v.p() - self.p();
        let inv_dist2: Float = 1.0 as Float / w.length_squared();
        w *= inv_dist2.sqrt();
        let mut pdf: Float = 0.0;
        if self.is_infinite_light() {
            // compute planar sampling density for infinite light sources
            let mut world_center: Point3f = Point3f::default();
            let mut world_radius: Float = 0.0;
            Bounds3f::bounding_sphere(&scene.world_bound(), &mut world_center, &mut world_radius);
            pdf = 1.0 as Float / (PI * world_radius * world_radius);
        } else {
            assert!(self.is_light());
            if self.vertex_type == VertexType::Light {
                if let Some(ref ei) = self.ei {
                    if let Some(ref light_ref) = ei.light {
                        // compute sampling density for non-infinite
                        // light sources
                        let mut pdf_pos: Float = 0.0;
                        let mut pdf_dir: Float = 0.0;
                        light_ref.pdf_le(
                            &Ray {
                                o: self.p(),
                                d: w,
                                t_max: Cell::new(std::f32::INFINITY),
                                time: self.time(),
                                differential: None,
                                medium: None,
                            },
                            &self.ng(),
                            &mut pdf_pos,
                            &mut pdf_dir,
                        );
                        pdf = pdf_dir * inv_dist2;
                    }
                }
            } else if let Some(ref si) = self.si {
                if let Some(primitive_raw) = si.primitive {
                    let primitive = unsafe { &*primitive_raw };
                    if let Some(area_light) = primitive.get_area_light() {
                        // compute sampling density for
                        // non-infinite light sources
                        let mut pdf_pos: Float = 0.0;
                        let mut pdf_dir: Float = 0.0;
                        area_light.pdf_le(
                            &Ray {
                                o: self.p(),
                                d: w,
                                t_max: Cell::new(std::f32::INFINITY),
                                time: self.time(),
                                differential: None,
                                medium: None,
                            },
                            &self.ng(),
                            &mut pdf_pos,
                            &mut pdf_dir,
                        );
                        pdf = pdf_dir * inv_dist2;
                    }
                }
            }
        }
        if v.is_on_surface() {
            pdf *= nrm_abs_dot_vec3f(&v.ng(), &w);
        }
        pdf
    }
    pub fn pdf_light_origin(
        &self,
        scene: &Scene,
        v: &Vertex,
        light_distr: Arc<Distribution1D>,
    ) -> Float {
        let mut w: Vector3f = v.p() - self.p();
        if w.length_squared() == 0.0 as Float {
            return 0.0 as Float;
        }
        w = w.normalize();
        if self.is_infinite_light() {
            // return solid angle density for infinite light sources
            return infinite_light_density(scene, light_distr, &w);
        } else {
            // return solid angle density for non-infinite light sources
            //         Float pdf_pos, pdf_dir, pdf_choice = 0;
            let mut pdf_pos: Float = 0.0;
            let mut pdf_dir: Float = 0.0;
            let mut pdf_choice: Float = 0.0;
            // get pointer _light_ to the light source at the vertex
            assert!(self.is_light());
            if self.vertex_type == VertexType::Light {
                // a real light source (not geometry emitting light)
                if let Some(ref ei) = self.ei {
                    if let Some(ref light_ref) = ei.light {
                        // find light in light vector
                        for i in 0..scene.lights.len() {
                            let light = &scene.lights[i];
                            // use ** (alloc::arc::Arc<Light> **)
                            let pr = &**light_ref as *const _ as *const usize;
                            let pl = &*light as *const _ as *const usize;
                            if pr == pl {
                                // compute the discrete probability of
                                // sampling _light_, _pdf_choice_
                                pdf_choice = light_distr.discrete_pdf(i);
                                light.pdf_le(
                                    &Ray {
                                        o: self.p(),
                                        d: w,
                                        t_max: Cell::new(std::f32::INFINITY),
                                        time: self.time(),
                                        differential: None,
                                        medium: None,
                                    },
                                    &self.ng(),
                                    &mut pdf_pos,
                                    &mut pdf_dir,
                                );
                                break;
                            }
                        }
                        return pdf_pos * pdf_choice;
                    }
                }
            } else {
                // area light from primitive
                if let Some(ref si) = self.si {
                    if let Some(primitive_raw) = si.primitive {
                        let primitive = unsafe { &*primitive_raw };
                        if let Some(area_light) = primitive.get_area_light() {
                            // find area light in light vector
                            for i in 0..scene.lights.len() {
                                let light = &scene.lights[i];
                                let pa = &*area_light as *const _ as *const usize;
                                let pl = &**light as *const _ as *const usize;
                                if pa == pl {
                                    // compute the discrete
                                    // probability of sampling
                                    // _light_, _pdf_choice_
                                    pdf_choice = light_distr.discrete_pdf(i);
                                    light.pdf_le(
                                        &Ray {
                                            o: self.p(),
                                            d: w,
                                            t_max: Cell::new(std::f32::INFINITY),
                                            time: self.time(),
                                            differential: None,
                                            medium: None,
                                        },
                                        &self.ng(),
                                        &mut pdf_pos,
                                        &mut pdf_dir,
                                    );
                                    break;
                                }
                            }
                            return pdf_pos * pdf_choice;
                        }
                    }
                }
            }
        }
        0.0 as Float
    }
}

/// Bidirectional Path Tracing (Global Illumination)
pub struct BDPTIntegrator {
    pub camera: Arc<Camera>,
    pub sampler: Box<Sampler>,
    pub pixel_bounds: Bounds2i,
    // see bdpt.h
    pub max_depth: u32,
    // visualize_strategies: bool,
    // visualize_weights: bool,
    pub light_sample_strategy: String, // "power"
}

impl BDPTIntegrator {
    pub fn new(
        camera: Arc<Camera>,
        sampler: Box<Sampler>,
        pixel_bounds: Bounds2i,
        max_depth: u32,
        // visualize_strategies: bool,
        // visualize_weights: bool,
        light_sample_strategy: String,
    ) -> Self {
        BDPTIntegrator {
            camera,
            sampler,
            pixel_bounds,
            max_depth,
            // visualize_strategies,
            // visualize_weights,
            light_sample_strategy,
        }
    }
    pub fn get_light_sample_strategy(&self) -> String {
        self.light_sample_strategy.clone()
    }

    pub fn render_tile<'a>(
        &self,
        x: u32,
        y: u32,
        n_x_tiles: i32,
        sample_bounds: Bounds2i,
        tile_size: i32,
        scene: &Scene,
        film: &'a Arc<Film>,
    ) -> FilmTile<'a> {
        let sampler = &self.get_sampler();
        let camera = &self.get_camera();
        let integrator = &self;

        let tile: Point2i = Point2i {
            x: x as i32,
            y: y as i32,
        };
        let seed: i32 = tile.y * n_x_tiles + tile.x;
        let mut tile_sampler: Box<Sampler> = sampler.clone_with_seed(seed as u64);
        let x0: i32 = sample_bounds.p_min.x + tile.x * tile_size;
        let x1: i32 = std::cmp::min(x0 + tile_size, sample_bounds.p_max.x);
        let y0: i32 = sample_bounds.p_min.y + tile.y * tile_size;
        let y1: i32 = std::cmp::min(y0 + tile_size, sample_bounds.p_max.y);
        let tile_bounds: Bounds2i =
            Bounds2i::new(Point2i { x: x0, y: y0 }, Point2i { x: x1, y: y1 });
        // println!("Starting image tile {:?}", tile_bounds);
        let mut film_tile = film.get_film_tile(&tile_bounds);
        for p_pixel in &tile_bounds {
            tile_sampler.start_pixel(p_pixel);
            if !pnt2_inside_exclusivei(p_pixel, &integrator.pixel_bounds) {
                continue;
            }
            let mut done: bool = false;
            while !done {
                // Get a distribution for sampling
                // the light at the start of the
                // light subpath. Because the
                // light path follows multiple
                // bounces, basing the sampling
                // distribution on any of the
                // vertices of the camera path is
                // unlikely to be a good
                // strategy. We use the
                // PowerLightDistribution by
                // default here, which doesn't use
                // the point passed to it. Now
                // trace the light subpath
                if let Some(light_distribution) =
                    create_light_sample_distribution(integrator.get_light_sample_strategy(), scene)
                {
                    // generate a single sample using BDPT
                    let p_film: Point2f = Point2f {
                        x: p_pixel.x as Float,
                        y: p_pixel.y as Float,
                    } + tile_sampler.get_2d();
                    // trace the camera subpath
                    let mut camera_vertices: Vec<Vertex> =
                        Vec::with_capacity((integrator.max_depth + 2) as usize);
                    let n_camera;
                    let p;
                    let time;
                    {
                        let (n_camera_new, p_new, time_new) = generate_camera_subpath(
                            scene,
                            &mut tile_sampler,
                            integrator.max_depth + 2,
                            camera,
                            p_film,
                            &mut camera_vertices,
                        );
                        n_camera = n_camera_new;
                        p = p_new;
                        time = time_new;
                    }
                    let light_distr: Arc<Distribution1D> = light_distribution.lookup(&p);
                    let mut light_vertices: Vec<Vertex> =
                        Vec::with_capacity((integrator.max_depth + 1) as usize);
                    let n_light;
                    {
                        n_light = generate_light_subpath(
                            scene,
                            &mut tile_sampler,
                            integrator.max_depth + 1,
                            time,
                            light_distr.clone(),
                            // light_to_index,
                            &mut light_vertices,
                        );
                    }
                    // Execute all BDPT connection strategies
                    let mut l: Spectrum = Spectrum::new(0.0 as Float);
                    // println!("n_camera = {:?}", n_camera);
                    // println!("n_light = {:?}", n_light);
                    for t in 1..=n_camera {
                        for s in 0..=n_light {
                            // int depth = t + s - 2;
                            let depth: isize = (t + s) as isize - 2;
                            if (s == 1 && t == 1)
                                || depth < 0
                                || depth > integrator.max_depth as isize
                            {
                                continue;
                            }
                            // execute the $(s, t)$ connection strategy and update _L_
                            let mut p_film_new: Point2f = Point2f {
                                x: p_film.x,
                                y: p_film.y,
                            };
                            let mut mis_weight: Option<Float> = Some(0.0 as Float);
                            let lpath: Spectrum = connect_bdpt(
                                scene,
                                &light_vertices,
                                &camera_vertices,
                                s,
                                t,
                                light_distr.clone(),
                                camera,
                                &mut tile_sampler,
                                &mut p_film_new,
                                mis_weight.as_mut(),
                            );
                            // if let Some(mis_weight_flt) = mis_weight {
                            //     println!("Connect bdpt s: {:?}, t: {:?}, lpath: {:?}, mis_weight: {:?}",
                            //              s, t, lpath, mis_weight_flt);
                            // }
                            // if (visualizeStrategies || visualizeWeights) {
                            //     Spectrum value;
                            //     if (visualizeStrategies)
                            //         value =
                            //             mis_weight == 0 ? 0 : lpath / mis_weight;
                            //     if (visualizeWeights) value = lpath;
                            //     weightFilms[BufferIndex(s, t)]->AddSplat(
                            //         pFilmNew, value);
                            // }
                            if t != 1 {
                                l += lpath;
                            } else if !lpath.is_black() {
                                film.add_splat(p_film_new, &lpath);
                            }
                        }
                    }
                    // println!(
                    //     "Add film sample pFilm: {:?}, L: {:?}, (y: {:?})",
                    //     p_film,
                    //     l,
                    //     l.y()
                    // );
                    film_tile.add_sample(p_film, &mut l, 1.0 as Float);
                    done = !tile_sampler.start_next_sample();
                }
            }
        }
        film_tile
    }

    pub fn render(
        &self,
        scene: &Scene,
        num_threads: u8,
        tile_size: i32,
        collector: bool,
        x_start: Option<u32>,
        y_start: Option<u32>,
        data: &str,
    ) -> Option<Vec<u8>> {
        // partition the image into tiles
        let film = self.get_camera().get_film();
        let sample_bounds: Bounds2i = film.get_sample_bounds();
        let sample_extent: Vector2i = sample_bounds.diagonal();
        let n_x_tiles: i32 = (sample_extent.x + tile_size - 1) / tile_size;
        let n_y_tiles: i32 = (sample_extent.y + tile_size - 1) / tile_size;
        // render and write the output image to disk
        if !scene.lights.is_empty() {
            if collector {
                let samples_per_pixel: i64 = self.sampler.get_samples_per_pixel();
                {
                    let block_queue = BlockQueue::new(
                        (
                            (n_x_tiles * tile_size) as u32,
                            (n_y_tiles * tile_size) as u32,
                        ),
                        (tile_size as u32, tile_size as u32),
                        (0, 0),
                    );
                    let bq = &block_queue;
                    let film = &film;
                    while let Some((x, y)) = bq.next() {
                        #[cfg(not(feature = "ecp"))]
                        #[cfg(not(test))]
                        unsafe {
                            http_request(x, y, tile_size, data.to_string());
                        }

                        #[cfg(test)]
                        {
                            let film_tile = self.render_tile(
                                x,
                                y,
                                n_x_tiles,
                                sample_bounds,
                                tile_size,
                                scene,
                                film,
                            );

                            // send the tile through the channel to main thread
                            film.merge_film_tile(&film_tile);
                        }
                    }
                }
            } else {
                let film = &film;
                let x = x_start.unwrap();
                let y = y_start.unwrap();
                let film_tile =
                    self.render_tile(x, y, n_x_tiles, sample_bounds, tile_size, scene, film);
                return Some(film.get_tile_image(
                    &film_tile,
                    tile_size,
                    x as i32,
                    y as i32,
                    sample_bounds,
                    1.0 as Float,
                ));
            }
            #[cfg(test)]
            film.write_image(1.0 as Float);
        }
        None
    }
    pub fn get_camera(&self) -> Arc<Camera> {
        self.camera.clone()
    }
    pub fn get_sampler(&self) -> &Sampler {
        &self.sampler
    }
}

// BDPT Utility Functions

pub fn correct_shading_normal(
    isect: &SurfaceInteraction,
    wo: &Vector3f,
    &wi: &Vector3f,
    mode: TransportMode,
) -> Float {
    if mode == TransportMode::Importance {
        let num: Float =
            vec3_abs_dot_nrmf(&wo, &isect.shading.n) * vec3_abs_dot_nrmf(&wi, &isect.common.n);
        let denom: Float =
            vec3_abs_dot_nrmf(&wo, &isect.common.n) * vec3_abs_dot_nrmf(&wi, &isect.shading.n);
        // wi is occasionally perpendicular to isect.shading.n; this
        // is fine, but we don't want to return an infinite or NaN
        // value in that case.
        if denom == 0.0 as Float {
            return 0.0 as Float;
        }
        num / denom
    } else {
        1.0 as Float
    }
}

pub fn generate_camera_subpath<'a>(
    scene: &'a Scene,
    sampler: &mut Sampler,
    max_depth: u32,
    camera: &'a Arc<Camera>,
    p_film: Point2f,
    path: &mut Vec<Vertex<'a>>,
) -> (usize, Point3f, Float) {
    if max_depth == 0 {
        return (0_usize, Point3f::default(), Float::default());
    }
    // TODO: ProfilePhase _(Prof::BDPTGenerateSubpath);
    // sample initial ray for camera subpath
    let mut camera_sample: CameraSample = CameraSample::default();
    camera_sample.p_film = p_film;
    camera_sample.time = sampler.get_1d();
    camera_sample.p_lens = sampler.get_2d();
    let mut ray: Ray = Ray::default();
    let mut beta: Spectrum =
        Spectrum::new(camera.generate_ray_differential(&camera_sample, &mut ray));
    ray.scale_differentials(1.0 as Float / (sampler.get_samples_per_pixel() as Float).sqrt());
    // generate first vertex on camera subpath and start random walk
    let vertex: Vertex = Vertex::create_camera_from_ray(camera, &ray, &beta);
    // get extra info
    let p: Point3f = vertex.p();
    let time: Float = vertex.time();
    // store vertex
    path.push(vertex);
    let (_pdf_pos, pdf_dir) = camera.pdf_we(&ray);
    (
        random_walk(
            scene,
            &ray,
            sampler,
            &mut beta,
            pdf_dir,
            max_depth - 1_u32,
            TransportMode::Radiance,
            path,
        ) + 1_usize,
        p,
        time,
    )
}

pub fn generate_light_subpath<'a>(
    scene: &'a Scene,
    sampler: &mut Sampler,
    max_depth: u32,
    time: Float,
    light_distr: Arc<Distribution1D>,
    // TODO: light_to_index
    path: &mut Vec<Vertex<'a>>,
) -> usize {
    let mut n_vertices: usize = 0_usize;
    if max_depth == 0_u32 {
        return 0_usize;
    }
    // TODO: ProfilePhase _(Prof::BDPTGenerateSubpath);
    // sample initial ray for light subpath
    let mut light_pdf: Option<Float> = Some(0.0 as Float);
    let light_num: usize = light_distr.sample_discrete(sampler.get_1d(), light_pdf.as_mut());
    let light = &scene.lights[light_num];
    let mut ray: Ray = Ray::default();
    let mut n_light: Normal3f = Normal3f::default();
    let mut pdf_pos: Float = 0.0 as Float;
    let mut pdf_dir: Float = 0.0 as Float;
    let u2: Point2f = sampler.get_2d();
    let u1: Point2f = sampler.get_2d();
    let le: Spectrum = light.sample_le(
        u1,
        u2,
        time,
        &mut ray,
        &mut n_light,
        &mut pdf_pos,
        &mut pdf_dir,
    );
    if pdf_pos == 0.0 as Float || pdf_dir == 0.0 as Float || le.is_black() {
        return 0_usize;
    }
    if let Some(light_pdf) = light_pdf {
        // generate first vertex on light subpath and start random walk
        let vertex: Vertex = Vertex::create_light(light, &ray, &n_light, &le, pdf_pos * light_pdf);
        let is_infinite_light: bool = vertex.is_infinite_light();
        path.push(vertex);
        let mut beta: Spectrum =
            le * nrm_abs_dot_vec3f(&n_light, &ray.d) / (light_pdf * pdf_pos * pdf_dir);
        // println!(
        //     "Starting light subpath. Ray: {:?}, Le {:?}, beta {:?}, pdf_pos {:?}, pdf_dir {:?}",
        //     ray, le, beta, pdf_pos, pdf_dir
        // );

        // set spatial density of _path[1]_ for infinite area
        // light is done in random_walk !!!
        n_vertices = random_walk(
            scene,
            &ray,
            sampler,
            &mut beta,
            pdf_dir,
            max_depth - 1,
            TransportMode::Importance,
            path,
        );
        // correct subpath sampling densities for infinite area lights
        if is_infinite_light {
            // set spatial density of _path[1]_ for infinite area light
            if n_vertices > 0 {
                path[1].pdf_fwd = pdf_pos;
                if path[1].is_on_surface() {
                    path[1].pdf_fwd *= vec3_abs_dot_nrmf(&ray.d, &path[1].ng());
                }
            }
            // set spatial density of _path[0]_ for infinite area light
            path[0].pdf_fwd = infinite_light_density(scene, light_distr, &ray.d);
        }
    }
    n_vertices + 1
}

pub fn random_walk<'a>(
    scene: &'a Scene,
    ray: &Ray,
    sampler: &mut Sampler,
    beta: &mut Spectrum,
    pdf: Float,
    max_depth: u32,
    mode: TransportMode,
    path: &mut Vec<Vertex<'a>>,
) -> usize {
    // create a copy of the ray which can be mutated
    let mut ray: Ray = ray.clone();
    let mut bounces: usize = 0_usize;
    if max_depth == 0_u32 {
        return bounces;
    }
    // declare variables for forward and reverse probability densities
    let mut pdf_fwd: Float = pdf;
    let mut pdf_rev: Float = 0.0;
    loop {
        // attempt to create the next subpath vertex in _path_
        // println!(
        //     "Random walk. Bounces {:?}, beta {:?}, pdf_fwd {:?}, pdf_rev {:?}",
        //     bounces, beta, pdf_fwd, pdf_rev
        // );
        let mut mi_opt: Option<MediumInteraction> = None;
        // trace a ray and sample the medium, if any
        let found_intersection: bool;
        let mut isect: SurfaceInteraction = SurfaceInteraction::default();
        if scene.intersect(&mut ray, &mut isect) {
            found_intersection = true;
        } else {
            found_intersection = false;
        }
        if let Some(ref medium) = ray.medium {
            let (spectrum, option) = medium.sample(&ray, sampler);
            *beta *= spectrum;
            if let Some(mi) = option {
                mi_opt = Some(mi);
            }
        }
        if beta.is_black() {
            break;
        }
        if let Some(mi) = mi_opt {
            // if mi.is_valid() {...}
            if let Some(phase) = mi.clone().phase {
                let vertex: Vertex;
                {
                    // record medium interaction in _path_ and compute forward density
                    let prev: &Vertex = &path[path.len() - 1];
                    vertex = Vertex::create_medium_interaction(mi, &beta, pdf_fwd, prev);
                }
                // if (++bounces >= maxDepth) break;
                bounces += 1;
                if bounces as u32 >= max_depth {
                    // store new vertex
                    path.push(vertex);
                    break;
                }
                // sample direction and compute reverse density at preceding vertex
                let mut wi: Vector3f = Vector3f::default();
                pdf_fwd = phase.sample_p(&(-ray.d), &mut wi, sampler.get_2d());
                pdf_rev = pdf_fwd;
                if let Some(ref mi) = vertex.mi {
                    let new_ray = mi.spawn_ray(&wi);
                    ray = new_ray;
                }
                // compute reverse area density at preceding vertex
                let new_pdf_rev;
                {
                    let prev: &Vertex = &path[path.len() - 1];
                    new_pdf_rev = vertex.convert_density(pdf_rev, prev);
                }
                let index: usize = path.len() - 1;
                path[index].pdf_rev = new_pdf_rev;
                // store new vertex
                path.push(vertex);
            }
        } else if !found_intersection {
            // capture escaped rays when tracing from the camera
            if mode == TransportMode::Radiance {
                let vertex: Vertex = Vertex::create_light_interaction(
                    EndpointInteraction::new_ray(&ray),
                    &beta,
                    pdf_fwd,
                );
                // store new vertex
                path.push(vertex);
                bounces += 1;
            }
            break;
        } else {
            // compute scattering functions for _mode_ and skip over medium
            // boundaries
            isect.compute_scattering_functions(&ray, true, mode);
            let isect_wo: Vector3f = isect.common.wo;
            let isect_shading_n: Normal3f = isect.shading.n;
            if isect.bsdf.is_none() {
                let new_ray = isect.spawn_ray(&ray.d);
                ray = new_ray;
                continue;
            }
            // initialize _vertex_ with surface intersection information
            let mut si_eval: SurfaceInteraction = SurfaceInteraction::default();
            si_eval.common.p = isect.common.p;
            si_eval.common.time = isect.common.time;
            si_eval.common.p_error = isect.common.p_error;
            si_eval.common.wo = isect.common.wo;
            si_eval.common.n = isect.common.n;
            if let Some(ref medium_interface) = isect.common.medium_interface {
                si_eval.common.medium_interface = Some(medium_interface.clone());
            } else {
                si_eval.common.medium_interface = None;
            }
            si_eval.uv = isect.uv;
            si_eval.dpdu = isect.dpdu;
            si_eval.dpdv = isect.dpdv;
            si_eval.dndu = isect.dndu;
            si_eval.dndv = isect.dndv;
            si_eval.dudx = Cell::new(isect.dudx.get());
            si_eval.dvdx = Cell::new(isect.dvdx.get());
            si_eval.dudy = Cell::new(isect.dudy.get());
            si_eval.dvdy = Cell::new(isect.dvdy.get());
            si_eval.dpdx = Cell::new(isect.dpdx.get());
            si_eval.dpdy = Cell::new(isect.dpdy.get());
            if let Some(primitive) = isect.primitive {
                si_eval.primitive = Some(primitive);
            } else {
                si_eval.primitive = None
            }
            si_eval.shading.n = isect.shading.n;
            si_eval.shading.dpdu = isect.shading.dpdu;
            si_eval.shading.dpdv = isect.shading.dpdv;
            si_eval.shading.dndu = isect.shading.dndu;
            si_eval.shading.dndv = isect.shading.dndv;
            if let Some(bsdf) = &isect.bsdf {
                si_eval.bsdf = Some(bsdf.clone());
            } else {
                si_eval.bsdf = None
            }
            // if let Some(bssrdf) = &isect.bssrdf {
            //     si_eval.bssrdf = Some(bssrdf.clone());
            // } else {
            //     si_eval.bssrdf = None
            // }
            if let Some(shape) = &isect.shape {
                si_eval.shape = Some(shape);
            } else {
                si_eval.shape = None
            }
            let mut vertex: Vertex =
                Vertex::create_surface_interaction(si_eval, &beta, pdf_fwd, &path[path.len() - 1]);
            bounces += 1;
            if bounces as u32 >= max_depth {
                // store new vertex
                path.push(vertex);
                break;
            }
            if let Some(ref bsdf) = isect.bsdf {
                // sample BSDF at current vertex and compute reverse probability
                let mut wi: Vector3f = Vector3f::default();
                let bsdf_flags: u8 = BxdfType::BsdfAll as u8;
                let mut sampled_type: u8 = u8::max_value(); // != 0
                let f: Spectrum = bsdf.sample_f(
                    &isect_wo,
                    &mut wi,
                    &sampler.get_2d(),
                    &mut pdf_fwd,
                    bsdf_flags,
                    &mut sampled_type,
                );
                // println!(
                //     "Random walk sampled dir {:?} f: {:?}, pdf_fwd: {:?}",
                //     wi, f, pdf_fwd
                // );
                if f.is_black() || pdf_fwd == 0.0 as Float {
                    // store new vertex
                    path.push(vertex);
                    break;
                }
                *beta *= f * vec3_abs_dot_nrmf(&wi, &isect_shading_n) / pdf_fwd;
                // println!("Random walk beta now {:?}", beta);
                pdf_rev = bsdf.pdf(&wi, &isect_wo, bsdf_flags);
                if (sampled_type & BxdfType::BsdfSpecular as u8) != 0_u8 {
                    vertex.delta = true;
                    pdf_rev = 0.0 as Float;
                    pdf_fwd = 0.0 as Float;
                }
                *beta *= Spectrum::new(correct_shading_normal(&isect, &isect_wo, &wi, mode));
                // println!(
                //     "Random walk beta after shading normal correction {:?}",
                //     beta
                // );
                let new_ray = isect.spawn_ray(&wi);
                ray = new_ray;
            }
            // compute reverse area density at preceding vertex
            let new_pdf_rev: Float;
            {
                let prev: &Vertex = &path[path.len() - 1];
                new_pdf_rev = vertex.convert_density(pdf_rev, prev);
            }
            let index: usize = path.len() - 1;
            path[index].pdf_rev = new_pdf_rev;
            // store new vertex
            path.push(vertex);
        }
    }
    assert!(
        bounces + 1 == path.len(),
        "bounces = {:?}, path.len = {:?}",
        bounces,
        path.len()
    );
    bounces
}

pub fn g<'a>(scene: &'a Scene, sampler: &mut Sampler, v0: &Vertex, v1: &Vertex) -> Spectrum {
    // Vector3f d = v0.p() - v1.p();
    let mut d: Vector3f = v0.p() - v1.p();
    let mut g: Float = 1.0 / d.length_squared();
    d *= g.sqrt();
    if v0.is_on_surface() {
        g *= nrm_abs_dot_vec3f(&v0.ns(), &d);
    }
    if v1.is_on_surface() {
        g *= nrm_abs_dot_vec3f(&v1.ns(), &d);
    }
    // VisibilityTester vis(v0.GetInteraction(), v1.GetInteraction());
    let mut p0: InteractionCommon = InteractionCommon::default();
    match v0.vertex_type {
        VertexType::Medium => {
            if let Some(ref mi) = v0.mi {
                p0 = mi.common.clone();
            }
        }
        VertexType::Surface => {
            if let Some(ref si) = v0.si {
                p0 = si.common.clone();
            }
        }
        _ => {
            if let Some(ref ei) = v0.ei {
                p0 = ei.common.clone();
            }
        }
    }
    let mut p1: InteractionCommon = InteractionCommon::default();
    match v1.vertex_type {
        VertexType::Medium => {
            if let Some(ref mi) = v1.mi {
                p1 = mi.common.clone();
            }
        }
        VertexType::Surface => {
            if let Some(ref si) = v1.si {
                p1 = si.common.clone();
            }
        }
        _ => {
            if let Some(ref ei) = v1.ei {
                p1 = ei.common.clone();
            }
        }
    }
    let vis: VisibilityTester = VisibilityTester {
        p0: Some(&p0),
        p1: Some(&p1),
    };
    vis.tr(scene, sampler) * g
}

pub fn mis_weight<'a>(
    scene: &'a Scene,
    light_vertices: &[Vertex<'a>],
    camera_vertices: &[Vertex<'a>],
    sampled: &Vertex,
    s: usize,
    t: usize,
    light_pdf: Arc<Distribution1D>,
) -> Float {
    if s + t == 2 as usize {
        return 1.0 as Float;
    }
    let mut sum_ri: Float = 0.0;
    // define helper function _remap0_ that deals with Dirac delta functions
    // auto remap0 = [](Float f) -> Float { return f != 0 ? f : 1; };

    // temporarily update vertex properties for current strategy

    // look up connection vertices and their predecessors
    // Vertex *qs = s > 0 ? &light_vertices[s - 1] : nullptr,
    //        *pt = t > 0 ? &camera_vertices[t - 1] : nullptr,
    //        *qsMinus = s > 1 ? &light_vertices[s - 2] : nullptr,
    //        *ptMinus = t > 1 ? &camera_vertices[t - 2] : nullptr;
    let mut qs: Option<Vertex> = None;
    let mut pt: Option<Vertex> = None;
    let mut qs_minus: Option<Vertex> = None;
    let mut pt_minus: Option<Vertex> = None;

    // update sampled vertex for $s=1$ or $t=1$ strategy
    if s == 1 {
        // a1 = {qs, sampled};
        let mut ei: Option<EndpointInteraction> = None;
        let mut mi: Option<MediumInteraction> = None;
        let mut si: Option<SurfaceInteraction> = None;
        if let Some(ref lv_ei) = sampled.ei {
            let mut medium_interface: Option<Arc<MediumInterface>> = None;
            let mut camera: Option<&Arc<Camera>> = None;
            let mut light: Option<&Arc<Light>> = None;
            if let Some(ref medium_interface_arc) = lv_ei.common.medium_interface {
                medium_interface = Some(medium_interface_arc.clone());
            }
            if let Some(camera_box) = lv_ei.camera {
                camera = Some(camera_box);
            }
            if let Some(light_arc) = lv_ei.light {
                light = Some(light_arc);
            }
            let mut common: InteractionCommon = InteractionCommon::default();
            common.p = lv_ei.common.p;
            common.time = lv_ei.common.time;
            common.p_error = lv_ei.common.p_error;
            common.wo = lv_ei.common.wo;
            common.n = lv_ei.common.n;
            common.medium_interface = medium_interface;
            let new_ei: EndpointInteraction = EndpointInteraction {
                common,
                camera,
                light,
            };
            ei = Some(new_ei);
        }
        if let Some(ref lv_mi) = sampled.mi {
            let mut medium_interface: Option<Arc<MediumInterface>> = None;
            let mut phase: Option<Arc<HenyeyGreenstein>> = None;
            if let Some(ref medium_interface_arc) = lv_mi.common.medium_interface {
                medium_interface = Some(medium_interface_arc.clone());
            }
            if let Some(ref phase_arc) = lv_mi.phase {
                phase = Some(phase_arc.clone());
            }
            let mut common: InteractionCommon = InteractionCommon::default();
            common.p = lv_mi.common.p;
            common.time = lv_mi.common.time;
            common.p_error = lv_mi.common.p_error;
            common.wo = lv_mi.common.wo;
            common.n = lv_mi.common.n;
            common.medium_interface = medium_interface;
            let new_mi: MediumInteraction = MediumInteraction { common, phase };
            mi = Some(new_mi);
        }
        if let Some(ref lv_si) = sampled.si {
            let mut medium_interface: Option<Arc<MediumInterface>> = None;
            if let Some(ref medium_interface_arc) = lv_si.common.medium_interface {
                medium_interface = Some(medium_interface_arc.clone());
            }
            let mut common: InteractionCommon = InteractionCommon::default();
            common.p = lv_si.common.p;
            common.time = lv_si.common.time;
            common.p_error = lv_si.common.p_error;
            common.wo = lv_si.common.wo;
            common.n = lv_si.common.n;
            common.medium_interface = medium_interface;
            if let Some(primitive) = lv_si.primitive {
                let new_si: SurfaceInteraction = SurfaceInteraction {
                    common,
                    primitive: Some(primitive),
                    bsdf: lv_si.bsdf.clone(),
                    ..Default::default()
                };
                si = Some(new_si);
            } else {
                let new_si: SurfaceInteraction = SurfaceInteraction {
                    common,
                    primitive: None,
                    bsdf: lv_si.bsdf.clone(),
                    ..Default::default()
                };
                si = Some(new_si);
            }
        }
        qs = Some(Vertex {
            vertex_type: sampled.vertex_type.clone(),
            beta: sampled.beta,
            ei,
            mi,
            si,
            delta: sampled.delta,
            pdf_fwd: sampled.pdf_fwd,
            pdf_rev: sampled.pdf_rev,
        });
    } else if t == 1 {
        // a1 = {pt, sampled};
        let mut ei: Option<EndpointInteraction> = None;
        let mut mi: Option<MediumInteraction> = None;
        let mut si: Option<SurfaceInteraction> = None;
        if let Some(ref lv_ei) = sampled.ei {
            let mut medium_interface: Option<Arc<MediumInterface>> = None;
            let mut camera: Option<&Arc<Camera>> = None;
            let mut light: Option<&Arc<Light>> = None;
            if let Some(ref medium_interface_arc) = lv_ei.common.medium_interface {
                medium_interface = Some(medium_interface_arc.clone());
            }
            if let Some(camera_box) = lv_ei.camera {
                camera = Some(camera_box);
            }
            if let Some(light_arc) = lv_ei.light {
                light = Some(light_arc);
            }
            let mut common: InteractionCommon = InteractionCommon::default();
            common.p = lv_ei.common.p;
            common.time = lv_ei.common.time;
            common.p_error = lv_ei.common.p_error;
            common.wo = lv_ei.common.wo;
            common.n = lv_ei.common.n;
            common.medium_interface = medium_interface;
            let new_ei: EndpointInteraction = EndpointInteraction {
                common,
                camera,
                light,
            };
            ei = Some(new_ei);
        }
        if let Some(ref lv_mi) = sampled.mi {
            let mut medium_interface: Option<Arc<MediumInterface>> = None;
            let mut phase: Option<Arc<HenyeyGreenstein>> = None;
            if let Some(ref medium_interface_arc) = lv_mi.common.medium_interface {
                medium_interface = Some(medium_interface_arc.clone());
            }
            if let Some(ref phase_arc) = lv_mi.phase {
                phase = Some(phase_arc.clone());
            }
            let mut common: InteractionCommon = InteractionCommon::default();
            common.p = lv_mi.common.p;
            common.time = lv_mi.common.time;
            common.p_error = lv_mi.common.p_error;
            common.wo = lv_mi.common.wo;
            common.n = lv_mi.common.n;
            common.medium_interface = medium_interface;
            let new_mi: MediumInteraction = MediumInteraction { common, phase };
            mi = Some(new_mi);
        }
        if let Some(ref lv_si) = sampled.si {
            let mut medium_interface: Option<Arc<MediumInterface>> = None;
            if let Some(ref medium_interface_arc) = lv_si.common.medium_interface {
                medium_interface = Some(medium_interface_arc.clone());
            }
            let mut common: InteractionCommon = InteractionCommon::default();
            common.p = lv_si.common.p;
            common.time = lv_si.common.time;
            common.p_error = lv_si.common.p_error;
            common.wo = lv_si.common.wo;
            common.n = lv_si.common.n;
            common.medium_interface = medium_interface;
            if let Some(primitive) = lv_si.primitive {
                let new_si: SurfaceInteraction = SurfaceInteraction {
                    common,
                    primitive: Some(primitive),
                    bsdf: lv_si.bsdf.clone(),
                    ..Default::default()
                };
                si = Some(new_si);
            } else {
                let new_si: SurfaceInteraction = SurfaceInteraction {
                    common,
                    primitive: None,
                    bsdf: lv_si.bsdf.clone(),
                    ..Default::default()
                };
                si = Some(new_si);
            }
        }
        pt = Some(Vertex {
            vertex_type: sampled.vertex_type.clone(),
            beta: sampled.beta,
            ei,
            mi,
            si,
            delta: sampled.delta,
            pdf_fwd: sampled.pdf_fwd,
            pdf_rev: sampled.pdf_rev,
        });
    }
    // mark connection vertices as non-degenerate
    if let Some(ref mut overwrite) = pt {
        overwrite.delta = false;
    } else if t > 0 {
        // *pt = t > 0 ? &cameraVertices[t - 1] : nullptr
        let mut ei: Option<EndpointInteraction> = None;
        let mut mi: Option<MediumInteraction> = None;
        let mut si: Option<SurfaceInteraction> = None;
        if let Some(ref cv_ei) = camera_vertices[t - 1].ei {
            let mut medium_interface: Option<Arc<MediumInterface>> = None;
            let mut camera: Option<&Arc<Camera>> = None;
            let mut light: Option<&Arc<Light>> = None;
            if let Some(ref medium_interface_arc) = cv_ei.common.medium_interface {
                medium_interface = Some(medium_interface_arc.clone());
            }
            if let Some(camera_box) = cv_ei.camera {
                camera = Some(camera_box);
            }
            if let Some(light_arc) = cv_ei.light {
                light = Some(light_arc);
            }
            let mut common: InteractionCommon = InteractionCommon::default();
            common.p = cv_ei.common.p;
            common.time = cv_ei.common.time;
            common.p_error = cv_ei.common.p_error;
            common.wo = cv_ei.common.wo;
            common.n = cv_ei.common.n;
            common.medium_interface = medium_interface;
            let new_ei: EndpointInteraction = EndpointInteraction {
                common,
                camera,
                light,
            };
            ei = Some(new_ei);
        }
        if let Some(ref cv_mi) = camera_vertices[t - 1].mi {
            let mut medium_interface: Option<Arc<MediumInterface>> = None;
            let mut phase: Option<Arc<HenyeyGreenstein>> = None;
            if let Some(ref medium_interface_arc) = cv_mi.common.medium_interface {
                medium_interface = Some(medium_interface_arc.clone());
            }
            if let Some(ref phase_arc) = cv_mi.phase {
                phase = Some(phase_arc.clone());
            }
            let mut common: InteractionCommon = InteractionCommon::default();
            common.p = cv_mi.common.p;
            common.time = cv_mi.common.time;
            common.p_error = cv_mi.common.p_error;
            common.wo = cv_mi.common.wo;
            common.n = cv_mi.common.n;
            common.medium_interface = medium_interface;
            let new_mi: MediumInteraction = MediumInteraction { common, phase };
            mi = Some(new_mi);
        }
        if let Some(ref cv_si) = camera_vertices[t - 1].si {
            let mut medium_interface: Option<Arc<MediumInterface>> = None;
            if let Some(ref medium_interface_arc) = cv_si.common.medium_interface {
                medium_interface = Some(medium_interface_arc.clone());
            }
            let mut common: InteractionCommon = InteractionCommon::default();
            common.p = cv_si.common.p;
            common.time = cv_si.common.time;
            common.p_error = cv_si.common.p_error;
            common.wo = cv_si.common.wo;
            common.n = cv_si.common.n;
            common.medium_interface = medium_interface;
            if let Some(primitive) = cv_si.primitive {
                let new_si: SurfaceInteraction = SurfaceInteraction {
                    common,
                    primitive: Some(primitive),
                    bsdf: cv_si.bsdf.clone(),
                    ..Default::default()
                };
                si = Some(new_si);
            } else {
                let new_si: SurfaceInteraction = SurfaceInteraction {
                    common,
                    primitive: None,
                    bsdf: cv_si.bsdf.clone(),
                    ..Default::default()
                };
                si = Some(new_si);
            }
        }
        pt = Some(Vertex {
            vertex_type: camera_vertices[t - 1].vertex_type.clone(),
            beta: camera_vertices[t - 1].beta,
            ei,
            mi,
            si,
            delta: false, // overwrite
            pdf_fwd: camera_vertices[t - 1].pdf_fwd,
            pdf_rev: camera_vertices[t - 1].pdf_rev,
        });
    }
    if let Some(ref mut overwrite) = qs {
        overwrite.delta = false;
    } else if s > 0 {
        // *qs = s > 0 ? &lightVertices[s - 1] : nullptr
        let mut ei: Option<EndpointInteraction> = None;
        let mut mi: Option<MediumInteraction> = None;
        let mut si: Option<SurfaceInteraction> = None;
        if let Some(ref lv_ei) = light_vertices[s - 1].ei {
            let mut medium_interface: Option<Arc<MediumInterface>> = None;
            let mut camera: Option<&Arc<Camera>> = None;
            let mut light: Option<&Arc<Light>> = None;
            if let Some(ref medium_interface_arc) = lv_ei.common.medium_interface {
                medium_interface = Some(medium_interface_arc.clone());
            }
            if let Some(camera_box) = lv_ei.camera {
                camera = Some(camera_box);
            }
            if let Some(light_arc) = lv_ei.light {
                light = Some(light_arc);
            }
            let mut common: InteractionCommon = InteractionCommon::default();
            common.p = lv_ei.common.p;
            common.time = lv_ei.common.time;
            common.p_error = lv_ei.common.p_error;
            common.wo = lv_ei.common.wo;
            common.n = lv_ei.common.n;
            common.medium_interface = medium_interface;
            let new_ei: EndpointInteraction = EndpointInteraction {
                common,
                camera,
                light,
            };
            ei = Some(new_ei);
        }
        if let Some(ref lv_mi) = light_vertices[s - 1].mi {
            let mut medium_interface: Option<Arc<MediumInterface>> = None;
            let mut phase: Option<Arc<HenyeyGreenstein>> = None;
            if let Some(ref medium_interface_arc) = lv_mi.common.medium_interface {
                medium_interface = Some(medium_interface_arc.clone());
            }
            if let Some(ref phase_arc) = lv_mi.phase {
                phase = Some(phase_arc.clone());
            }
            let mut common: InteractionCommon = InteractionCommon::default();
            common.p = lv_mi.common.p;
            common.time = lv_mi.common.time;
            common.p_error = lv_mi.common.p_error;
            common.wo = lv_mi.common.wo;
            common.n = lv_mi.common.n;
            common.medium_interface = medium_interface;
            let new_mi: MediumInteraction = MediumInteraction { common, phase };
            mi = Some(new_mi);
        }
        if let Some(ref lv_si) = light_vertices[s - 1].si {
            let mut medium_interface: Option<Arc<MediumInterface>> = None;
            if let Some(ref medium_interface_arc) = lv_si.common.medium_interface {
                medium_interface = Some(medium_interface_arc.clone());
            }
            let mut common: InteractionCommon = InteractionCommon::default();
            common.p = lv_si.common.p;
            common.time = lv_si.common.time;
            common.p_error = lv_si.common.p_error;
            common.wo = lv_si.common.wo;
            common.n = lv_si.common.n;
            common.medium_interface = medium_interface;
            if let Some(primitive) = lv_si.primitive {
                let new_si: SurfaceInteraction = SurfaceInteraction {
                    common,
                    primitive: Some(primitive),
                    bsdf: lv_si.bsdf.clone(),
                    ..Default::default()
                };
                si = Some(new_si);
            } else {
                let new_si: SurfaceInteraction = SurfaceInteraction {
                    common,
                    primitive: None,
                    bsdf: lv_si.bsdf.clone(),
                    ..Default::default()
                };
                si = Some(new_si);
            }
        }
        qs = Some(Vertex {
            vertex_type: light_vertices[s - 1].vertex_type.clone(),
            beta: light_vertices[s - 1].beta,
            ei,
            mi,
            si,
            delta: false, // overwrite
            pdf_fwd: light_vertices[s - 1].pdf_fwd,
            pdf_rev: light_vertices[s - 1].pdf_rev,
        });
    }

    // update reverse density of vertex $\pt{}_{t-1}$
    if let Some(ref mut overwrite) = pt {
        if s > 0 {
            if let Some(ref callable) = qs {
                if s > 1 {
                    overwrite.pdf_rev =
                        callable.pdf(scene, Some(&light_vertices[s - 2]), &overwrite);
                } else {
                    overwrite.pdf_rev = callable.pdf(scene, None, &overwrite);
                }
            }
        } else if t > 1 {
            overwrite.pdf_rev =
                overwrite.pdf_light_origin(scene, &camera_vertices[t - 2], light_pdf);
        }
    }
    // update reverse density of vertex $\pt{}_{t-2}$
    if let Some(ref callable) = pt {
        if t > 1 {
            let mut ei: Option<EndpointInteraction> = None;
            let mut mi: Option<MediumInteraction> = None;
            let mut si: Option<SurfaceInteraction> = None;
            if let Some(ref cv_ei) = camera_vertices[t - 2].ei {
                let mut medium_interface: Option<Arc<MediumInterface>> = None;
                let mut camera: Option<&Arc<Camera>> = None;
                let mut light: Option<&Arc<Light>> = None;
                if let Some(ref medium_interface_arc) = cv_ei.common.medium_interface {
                    medium_interface = Some(medium_interface_arc.clone());
                }
                if let Some(camera_box) = cv_ei.camera {
                    camera = Some(camera_box);
                }
                if let Some(light_arc) = cv_ei.light {
                    light = Some(light_arc);
                }
                let mut common: InteractionCommon = InteractionCommon::default();
                common.p = cv_ei.common.p;
                common.time = cv_ei.common.time;
                common.p_error = cv_ei.common.p_error;
                common.wo = cv_ei.common.wo;
                common.n = cv_ei.common.n;
                common.medium_interface = medium_interface;
                let new_ei: EndpointInteraction = EndpointInteraction {
                    common,
                    camera,
                    light,
                };
                ei = Some(new_ei);
            }
            if let Some(ref cv_mi) = camera_vertices[t - 2].mi {
                let mut medium_interface: Option<Arc<MediumInterface>> = None;
                let mut phase: Option<Arc<HenyeyGreenstein>> = None;
                if let Some(ref medium_interface_arc) = cv_mi.common.medium_interface {
                    medium_interface = Some(medium_interface_arc.clone());
                }
                if let Some(ref phase_arc) = cv_mi.phase {
                    phase = Some(phase_arc.clone());
                }
                let mut common: InteractionCommon = InteractionCommon::default();
                common.p = cv_mi.common.p;
                common.time = cv_mi.common.time;
                common.p_error = cv_mi.common.p_error;
                common.wo = cv_mi.common.wo;
                common.n = cv_mi.common.n;
                common.medium_interface = medium_interface;
                let new_mi: MediumInteraction = MediumInteraction { common, phase };
                mi = Some(new_mi);
            }
            if let Some(ref cv_si) = camera_vertices[t - 2].si {
                let mut medium_interface: Option<Arc<MediumInterface>> = None;
                if let Some(ref medium_interface_arc) = cv_si.common.medium_interface {
                    medium_interface = Some(medium_interface_arc.clone());
                }
                let mut common: InteractionCommon = InteractionCommon::default();
                common.p = cv_si.common.p;
                common.time = cv_si.common.time;
                common.p_error = cv_si.common.p_error;
                common.wo = cv_si.common.wo;
                common.n = cv_si.common.n;
                common.medium_interface = medium_interface;
                let new_si: SurfaceInteraction = SurfaceInteraction {
                    common,
                    bsdf: cv_si.bsdf.clone(),
                    ..Default::default()
                };
                si = Some(new_si);
            }
            let pdf_rev;
            if s > 0 {
                if let Some(ref qs_ref) = qs {
                    pdf_rev = callable.pdf(scene, Some(&qs_ref), &camera_vertices[t - 2]);
                } else {
                    pdf_rev = callable.pdf(scene, None, &camera_vertices[t - 2]);
                }
            } else {
                pdf_rev = callable.pdf_light(scene, &camera_vertices[t - 2]);
            }
            pt_minus = Some(Vertex {
                vertex_type: camera_vertices[t - 2].vertex_type.clone(),
                beta: camera_vertices[t - 2].beta,
                ei,
                mi,
                si,
                delta: camera_vertices[t - 2].delta,
                pdf_fwd: camera_vertices[t - 2].pdf_fwd,
                pdf_rev,
            });
        }
    }

    // update reverse density of vertices $\pq{}_{s-1}$ and $\pq{}_{s-2}$
    if let Some(ref mut overwrite) = qs {
        if let Some(ref callable) = pt {
            if let Some(ref pt_ref) = pt_minus {
                overwrite.pdf_rev = callable.pdf(scene, Some(&pt_ref), &overwrite);
            } else {
                overwrite.pdf_rev = callable.pdf(scene, None, &overwrite);
            }
        }
    }
    if let Some(ref callable) = qs {
        if s > 1 {
            let mut ei: Option<EndpointInteraction> = None;
            let mut mi: Option<MediumInteraction> = None;
            let mut si: Option<SurfaceInteraction> = None;
            if let Some(ref lv_ei) = light_vertices[s - 2].ei {
                let mut medium_interface: Option<Arc<MediumInterface>> = None;
                let mut camera: Option<&Arc<Camera>> = None;
                let mut light: Option<&Arc<Light>> = None;
                if let Some(ref medium_interface_arc) = lv_ei.common.medium_interface {
                    medium_interface = Some(medium_interface_arc.clone());
                }
                if let Some(camera_box) = lv_ei.camera {
                    camera = Some(camera_box);
                }
                if let Some(light_arc) = lv_ei.light {
                    light = Some(light_arc);
                }
                let mut common: InteractionCommon = InteractionCommon::default();
                common.p = lv_ei.common.p;
                common.time = lv_ei.common.time;
                common.p_error = lv_ei.common.p_error;
                common.wo = lv_ei.common.wo;
                common.n = lv_ei.common.n;
                common.medium_interface = medium_interface;
                let new_ei: EndpointInteraction = EndpointInteraction {
                    common,
                    camera,
                    light,
                };
                ei = Some(new_ei);
            }
            if let Some(ref lv_mi) = light_vertices[s - 2].mi {
                let mut medium_interface: Option<Arc<MediumInterface>> = None;
                let mut phase: Option<Arc<HenyeyGreenstein>> = None;
                if let Some(ref medium_interface_arc) = lv_mi.common.medium_interface {
                    medium_interface = Some(medium_interface_arc.clone());
                }
                if let Some(ref phase_arc) = lv_mi.phase {
                    phase = Some(phase_arc.clone());
                }
                let mut common: InteractionCommon = InteractionCommon::default();
                common.p = lv_mi.common.p;
                common.time = lv_mi.common.time;
                common.p_error = lv_mi.common.p_error;
                common.wo = lv_mi.common.wo;
                common.n = lv_mi.common.n;
                common.medium_interface = medium_interface;
                let new_mi: MediumInteraction = MediumInteraction { common, phase };
                mi = Some(new_mi);
            }
            if let Some(ref lv_si) = light_vertices[s - 2].si {
                let mut medium_interface: Option<Arc<MediumInterface>> = None;
                if let Some(ref medium_interface_arc) = lv_si.common.medium_interface {
                    medium_interface = Some(medium_interface_arc.clone());
                }
                let mut common: InteractionCommon = InteractionCommon::default();
                common.p = lv_si.common.p;
                common.time = lv_si.common.time;
                common.p_error = lv_si.common.p_error;
                common.wo = lv_si.common.wo;
                common.n = lv_si.common.n;
                common.medium_interface = medium_interface;
                let new_si: SurfaceInteraction = SurfaceInteraction {
                    common,
                    bsdf: lv_si.bsdf.clone(),
                    ..Default::default()
                };
                si = Some(new_si);
            }
            let pdf_rev;
            if let Some(ref pt_ref) = pt {
                pdf_rev = callable.pdf(scene, Some(&pt_ref), &light_vertices[s - 2]);
            } else {
                pdf_rev = callable.pdf(scene, None, &light_vertices[s - 2]);
            }
            qs_minus = Some(Vertex {
                vertex_type: light_vertices[s - 2].vertex_type.clone(),
                beta: light_vertices[s - 2].beta,
                ei,
                mi,
                si,
                delta: light_vertices[s - 2].delta,
                pdf_fwd: light_vertices[s - 2].pdf_fwd,
                pdf_rev,
            });
        }
    }

    // consider hypothetical connection strategies along the camera subpath
    let mut ri: Float = 1.0;
    let mut i: usize = t - 1;
    while i > 0 {
        let mut cv1: &Vertex = &camera_vertices[i];
        let mut cv0: &Vertex = &camera_vertices[i - 1];
        if i == t - 1 {
            // use modified camera vertices
            if let Some(ref cv) = pt {
                cv1 = cv;
            }
            if let Some(ref cv) = pt_minus {
                cv0 = cv;
            }
        } else if i == t - 2 {
            // use modified camera vertex
            if let Some(ref cv) = pt_minus {
                cv1 = cv;
            }
        }
        let mut numerator: Float = cv1.pdf_rev;
        if numerator == 0.0 {
            numerator = 1.0;
        }
        let mut denominator: Float = cv1.pdf_fwd;
        if denominator == 0.0 {
            denominator = 1.0;
        }
        ri *= numerator / denominator;
        if !cv1.delta && !cv0.delta {
            sum_ri += ri;
        }
        i -= 1;
    }

    // consider hypothetical connection strategies along the light subpath
    ri = 1.0 as Float;
    let mut i: isize = s as isize - 1;
    while i >= 0 {
        let mut lv1: &Vertex = &light_vertices[i as usize];
        if i == s as isize - 1 {
            // use modified light vertices
            if let Some(ref lv) = qs {
                lv1 = lv;
            }
        } else if i == s as isize - 2 {
            // use modified light vertex
            if let Some(ref lv) = qs_minus {
                lv1 = lv;
            }
        }
        let mut numerator: Float = lv1.pdf_rev;
        if numerator == 0.0 {
            numerator = 1.0;
        }
        let mut denominator: Float = lv1.pdf_fwd;
        if denominator == 0.0 {
            denominator = 1.0;
        }
        ri *= numerator / denominator;
        let delta_lightvertex: bool;
        if i > 0 {
            if i == s as isize - 1 {
                // i - 1 == s - 2 (qs_minus == light_vertices[s - 2])
                if let Some(ref lv) = qs_minus {
                    // use modified light vertex
                    delta_lightvertex = lv.delta;
                } else {
                    delta_lightvertex = light_vertices[(i - 1) as usize].delta;
                }
            } else {
                delta_lightvertex = light_vertices[(i - 1) as usize].delta;
            }
        } else {
            delta_lightvertex = lv1.is_delta_light();
        }
        if !lv1.delta && !delta_lightvertex {
            sum_ri += ri;
        }
        i -= 1;
    }
    1.0 as Float / (1.0 as Float + sum_ri)
}

pub fn connect_bdpt<'a>(
    scene: &'a Scene,
    light_vertices: &[Vertex<'a>],
    camera_vertices: &[Vertex<'a>],
    s: usize,
    t: usize,
    light_distr: Arc<Distribution1D>,
    camera: &'a Arc<Camera>,
    sampler: &mut Sampler,
    p_raster: &mut Point2f,
    mis_weight_opt: Option<&mut Float>,
) -> Spectrum {
    // TODO: ProfilePhase _(Prof::BDPTConnectSubpaths);
    let mut l: Spectrum = Spectrum::default();
    // ignore invalid connections related to infinite area lights
    if t > 1 && s != 0 && camera_vertices[t - 1].vertex_type == VertexType::Light {
        return Spectrum::default();
    }
    // perform connection and write contribution to _L_
    let mut sampled: Vertex = Vertex {
        vertex_type: VertexType::Medium,
        beta: Spectrum::default(),
        ei: None,
        mi: None,
        si: None,
        delta: false,
        pdf_fwd: 0.0 as Float,
        pdf_rev: 0.0 as Float,
    };
    if s == 0 {
        // interpret the camera subpath as a complete path
        if camera_vertices[t - 1].is_light() {
            l = camera_vertices[t - 1].le(scene, &camera_vertices[t - 2])
                * camera_vertices[t - 1].beta;
        }
        assert!(!l.has_nans());
    } else if t == 1 {
        // sample a point on the camera and connect it to the light subpath
        assert!(
            (s - 1) < light_vertices.len(),
            "(s - 1) = {:?} should be less than length of light vertices({:?})",
            (s - 1),
            light_vertices.len()
        );
        if light_vertices[s - 1].is_connectible() {
            let mut iref: InteractionCommon = InteractionCommon::default();
            let mut lens_intr: InteractionCommon = InteractionCommon::default();
            // qs.GetInteraction()
            match light_vertices[s - 1].vertex_type {
                VertexType::Medium => {
                    if let Some(ref mi) = light_vertices[s - 1].mi {
                        iref = mi.common.clone();
                    } else {
                    }
                }
                VertexType::Surface => {
                    if let Some(ref si) = light_vertices[s - 1].si {
                        iref = si.common.clone();
                    } else {
                    }
                }
                _ => {
                    if let Some(ref ei) = light_vertices[s - 1].ei {
                        iref = ei.common.clone();
                    } else {
                    }
                }
            }
            let mut wi: Vector3f = Vector3f::default();
            let mut pdf: Float = 0.0 as Float;
            let mut vis: VisibilityTester = VisibilityTester::default();
            let wi_color: Spectrum = camera.sample_wi(
                &iref,
                &mut lens_intr,
                sampler.get_2d(),
                &mut wi,
                &mut pdf,
                p_raster,
                &mut vis,
            );
            if pdf > 0.0 as Float && !wi_color.is_black() {
                // initialize dynamically sampled vertex and _L_ for $t=1$ case
                sampled = Vertex::create_camera_from_interaction(
                    camera,
                    vis.p1.as_ref().unwrap().clone(),
                    &(wi_color / pdf),
                );
                l = light_vertices[s - 1].beta
                    * light_vertices[s - 1].f(&sampled, TransportMode::Importance)
                    * sampled.beta;
                // println!("l = {:?}", l);
                if light_vertices[s - 1].is_on_surface() {
                    l *= Spectrum::new(vec3_abs_dot_nrmf(&wi, &light_vertices[s - 1].ns()));
                }
                assert!(!l.has_nans());
                // only check visibility after we know that the path
                // would make a non-zero contribution.
                if !l.is_black() {
                    l *= vis.tr(scene, sampler);
                }
            }
        }
    } else if s == 1 {
        // sample a point on a light and connect it to the camera subpath
        if camera_vertices[t - 1].is_connectible() {
            let mut light_intr: InteractionCommon = InteractionCommon::default();
            let mut wi: Vector3f = Vector3f::default();
            let mut pdf: Float = 0.0 as Float;
            let mut light_pdf: Option<Float> = Some(0.0 as Float);
            let mut vis: VisibilityTester = VisibilityTester::default();
            let light_num: usize =
                light_distr.sample_discrete(sampler.get_1d(), light_pdf.as_mut());
            //         const std::shared_ptr<Light> &light = scene.lights[light_num];
            let mut iref: InteractionCommon = InteractionCommon::default();
            // pt.GetInteraction()
            match camera_vertices[t - 1].vertex_type {
                VertexType::Medium => {
                    if let Some(ref mi) = camera_vertices[t - 1].mi {
                        iref.p = mi.common.p;
                        iref.time = mi.common.time;
                        iref.p_error = mi.common.p_error;
                        iref.wo = mi.common.wo;
                        iref.n = mi.common.n;
                        if let Some(ref medium_interface_arc) = mi.common.medium_interface {
                            iref.medium_interface = Some(medium_interface_arc.clone())
                        }
                    } else {
                    }
                }
                VertexType::Surface => {
                    if let Some(ref si) = camera_vertices[t - 1].si {
                        iref.p = si.common.p;
                        iref.time = si.common.time;
                        iref.p_error = si.common.p_error;
                        iref.wo = si.common.wo;
                        iref.n = si.common.n;
                    } else {
                    }
                }
                _ => {
                    if let Some(ref ei) = camera_vertices[t - 1].ei {
                        iref.p = ei.common.p;
                        iref.time = ei.common.time;
                        iref.p_error = ei.common.p_error;
                        iref.wo = ei.common.wo;
                        iref.n = ei.common.n;
                    } else {
                    }
                }
            }
            let light_weight: Spectrum = scene.lights[light_num].sample_li(
                &iref,
                &mut light_intr,
                sampler.get_2d(),
                &mut wi,
                &mut pdf,
                &mut vis,
            );
            if pdf > 0.0 as Float && !light_weight.is_black() {
                let ei: EndpointInteraction = EndpointInteraction::new_interaction_from_light(
                    vis.p1.as_ref().unwrap().clone(),
                    &scene.lights[light_num],
                );
                sampled = Vertex::create_light_interaction(
                    ei,
                    &(light_weight / (pdf * light_pdf.unwrap())),
                    0.0 as Float,
                );
                sampled.pdf_fwd =
                    sampled.pdf_light_origin(scene, &camera_vertices[t - 1], light_distr.clone());
                l = camera_vertices[t - 1].beta
                    * camera_vertices[t - 1].f(&sampled, TransportMode::Radiance)
                    * sampled.beta;
                if camera_vertices[t - 1].is_on_surface() {
                    l *= Spectrum::new(vec3_abs_dot_nrmf(&wi, &camera_vertices[t - 1].ns()));
                }
                // only check visibility if the path would carry radiance.
                if !l.is_black() {
                    l *= vis.tr(scene, sampler);
                }
            }
        }
    } else {
        // handle all other bidirectional connection cases
        if light_vertices[s - 1].is_connectible() && camera_vertices[t - 1].is_connectible() {
            let radiance =
                camera_vertices[t - 1].f(&light_vertices[s - 1], TransportMode::Radiance);
            let importance =
                light_vertices[s - 1].f(&camera_vertices[t - 1], TransportMode::Importance);
            l = light_vertices[s - 1].beta * importance * radiance * camera_vertices[t - 1].beta;
            // print!("General connect s: {:?}, t: {:?} ", s, t);
            // print!(
            //     "qs: {:?}, pt: {:?}, ",
            //     light_vertices[s - 1],
            //     camera_vertices[t - 1]
            // );
            // print!(
            //     "qs.f(pt): {:?}, ",
            //     light_vertices[s - 1].f(camera_vertices[t - 1], TransportMode::Importance)
            // );
            // print!(
            //     "pt.f(qs): {:?}, ",
            //     camera_vertices[t - 1].f(light_vertices[s - 1], TransportMode::Radiance)
            // );
            // print!(
            //     "G: {:?}, ",
            //     g(
            //         scene,
            //         sampler,
            //         light_vertices[s - 1],
            //         camera_vertices[t - 1]
            //     )
            // );
            // println!(
            //     "dist^2: {:?}",
            //     distance_squared(light_vertices[s - 1].p(), camera_vertices[t - 1].p())
            // );
            if !l.is_black() {
                l *= g(
                    scene,
                    sampler,
                    &light_vertices[s - 1],
                    &camera_vertices[t - 1],
                );
            }
        }
    }
    // TODO:
    // ++totalPaths;
    // if (L.IsBlack()) ++zeroRadiancePaths;
    // ReportValue(pathLength, s + t - 2);

    // compute MIS weight for connection strategy
    let mis_weight_flt = if !l.is_black() {
        mis_weight(
            scene,
            light_vertices,
            camera_vertices,
            &sampled,
            s,
            t,
            light_distr,
        )
    } else {
        0.0 as Float
    };
    // if mis_weight_flt > 0.0 {
    //     println!(
    //         "MIS weight for (s,t) = ({:?}, {:?}) connection: {:?}",
    //         s, t, mis_weight_flt
    //     );
    // }
    assert!(!mis_weight_flt.is_nan());
    l *= Spectrum::new(mis_weight_flt);
    if let Some(mis_weight_ptr) = mis_weight_opt {
        *mis_weight_ptr = mis_weight_flt;
    }
    l
}

pub fn infinite_light_density<'a>(
    scene: &'a Scene,
    light_distr: Arc<Distribution1D>,
    // const std::unordered_map<const Light *, size_t> &lightToDistrIndex,
    w: &Vector3f,
) -> Float {
    let mut pdf: Float = 0.0 as Float;
    for light in &scene.infinite_lights {
        // for i in 0..scene.infinite_lights.len() {
        //     CHECK(lightToDistrIndex.find(light.get()) != lightToDistrIndex.end());
        //     size_t index = lightToDistrIndex.find(light.get())->second;
        let index: usize = 0; // TODO: calculate index (see above)
        pdf += light.pdf_li(&SurfaceInteraction::default(), &-(*w)) * light_distr.func[index];
    }
    // TODO: Old loop (without cache) !!!
    // for (size_t i = 0; i < scene.lights.size(); ++i)
    //     if (scene.lights[i]->flags & (int)LightFlags::Infinite)
    //         pdf +=
    //             scene.lights[i]->Pdf_Li(Interaction(), -w) * light_distr.func[i];
    pdf / (light_distr.func_int * light_distr.count() as Float)
}
