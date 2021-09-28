// std
use std::f32::consts::PI;
use std::sync::Arc;
// pbrt
use crate::core::efloat::quadratic_efloat;
use crate::core::efloat::EFloat;
use crate::core::geometry::{
    nrm_abs_dot_vec3f, pnt3_distance_squaredf, pnt3_distancef, pnt3_offset_ray_origin,
    spherical_direction_vec3, vec3_coordinate_system, vec3_cross_vec3, vec3_dot_vec3f,
};
use crate::core::geometry::{Bounds3f, Normal3f, Point2f, Point3f, Ray, Vector3f, XYEnum};
use crate::core::interaction::{Interaction, InteractionCommon, SurfaceInteraction};
use crate::core::material::Material;
use crate::core::pbrt::Float;
use crate::core::pbrt::{clamp_t, gamma, radians};
use crate::core::sampling::{uniform_cone_pdf, uniform_sample_sphere};
use crate::core::transform::Transform;

// see sphere.h

#[derive(Clone, Serialize, Deserialize)]
pub struct Sphere {
    pub radius: Float,
    pub z_min: Float,
    pub z_max: Float,
    pub theta_min: Float,
    pub theta_max: Float,
    pub phi_max: Float,
    // inherited from class Shape (see shape.h)
    pub object_to_world: Transform,
    pub world_to_object: Transform,
    pub reverse_orientation: bool,
    pub transform_swaps_handedness: bool,
    pub material: Option<Arc<Material>>,
}

impl Default for Sphere {
    fn default() -> Self {
        let object_to_world: Transform = Transform::default();
        Sphere {
            // Shape
            object_to_world,
            world_to_object: Transform::default(),
            reverse_orientation: false,
            transform_swaps_handedness: object_to_world.swaps_handedness(),
            // Sphere
            radius: 1.0,
            z_min: -1.0,
            z_max: 1.0,
            theta_min: (-1.0 as Float).acos(),
            theta_max: (1.0 as Float).acos(),
            phi_max: radians(360.0),
            material: None,
        }
    }
}

impl Sphere {
    pub fn new(
        object_to_world: Transform,
        world_to_object: Transform,
        reverse_orientation: bool,
        radius: Float,
        z_min: Float,
        z_max: Float,
        phi_max: Float,
    ) -> Self {
        Sphere {
            // Shape
            object_to_world,
            world_to_object,
            reverse_orientation,
            transform_swaps_handedness: object_to_world.swaps_handedness(),
            // Sphere
            radius,
            z_min: clamp_t(z_min.min(z_max), -radius, radius),
            z_max: clamp_t(z_min.max(z_max), -radius, radius),
            theta_min: clamp_t(z_min.min(z_max) / radius, -1.0, 1.0).acos(),
            theta_max: clamp_t(z_min.max(z_max) / radius, -1.0, 1.0).acos(),
            phi_max: radians(clamp_t(phi_max, 0.0, 360.0)),
            material: None,
        }
    }
    // Shape
    pub fn object_bound(&self) -> Bounds3f {
        Bounds3f {
            p_min: Point3f {
                x: -self.radius,
                y: -self.radius,
                z: self.z_min,
            },
            p_max: Point3f {
                x: self.radius,
                y: self.radius,
                z: self.z_max,
            },
        }
    }
    pub fn world_bound(&self) -> Bounds3f {
        // in C++: Bounds3f Shape::WorldBound() const { return (*ObjectToWorld)(ObjectBound()); }
        self.object_to_world.transform_bounds(&self.object_bound())
    }
    pub fn intersect(&self, r: &Ray, t_hit: &mut Float, isect: &mut SurfaceInteraction) -> bool {
        // transform _Ray_ to object space
        let mut o_err: Vector3f = Vector3f::default();
        let mut d_err: Vector3f = Vector3f::default();
        let ray: Ray = self
            .world_to_object
            .transform_ray_with_error(r, &mut o_err, &mut d_err);

        // compute quadratic sphere coefficients

        // initialize _EFloat_ ray coordinate values
        let ox = EFloat::new(ray.o.x as f32, o_err.x as f32);
        let oy = EFloat::new(ray.o.y as f32, o_err.y as f32);
        let oz = EFloat::new(ray.o.z as f32, o_err.z as f32);
        let dx = EFloat::new(ray.d.x as f32, d_err.x as f32);
        let dy = EFloat::new(ray.d.y as f32, d_err.y as f32);
        let dz = EFloat::new(ray.d.z as f32, d_err.z as f32);
        let a: EFloat = dx * dx + dy * dy + dz * dz;
        let b: EFloat = (dx * ox + dy * oy + dz * oz) * 2.0f32;
        let c: EFloat = ox * ox + oy * oy + oz * oz
            - EFloat::new(self.radius as f32, 0.0) * EFloat::new(self.radius as f32, 0.0);

        // solve quadratic equation for _t_ values
        let mut t0: EFloat = EFloat::default();
        let mut t1: EFloat = EFloat::default();
        if !quadratic_efloat(a, b, c, &mut t0, &mut t1) {
            return false;
        }
        // check quadric shape _t0_ and _t1_ for nearest intersection
        if t0.upper_bound() > ray.t_max.get() as f32 || t1.lower_bound() <= 0.0f32 {
            return false;
        }
        let mut t_shape_hit: EFloat = t0;
        if t_shape_hit.lower_bound() <= 0.0f32 {
            t_shape_hit = t1;
            if t_shape_hit.upper_bound() > ray.t_max.get() as f32 {
                return false;
            }
        }
        // compute sphere hit position and $\phi$
        let mut p_hit: Point3f = ray.position(t_shape_hit.v);
        // refine sphere intersection point
        p_hit *= self.radius / pnt3_distancef(&p_hit, &Point3f::default());
        if p_hit.x == 0.0 && p_hit.y == 0.0 {
            p_hit.x = 1e-5_f32 * self.radius;
        }
        let mut phi: Float = p_hit.y.atan2(p_hit.x);
        if phi < 0.0 {
            phi += 2.0_f32 * PI;
        }
        // test sphere intersection against clipping parameters
        if (self.z_min > -self.radius && p_hit.z < self.z_min)
            || (self.z_max < self.radius && p_hit.z > self.z_max)
            || phi > self.phi_max
        {
            if t_shape_hit == t1 {
                return false;
            }
            if t1.upper_bound() > ray.t_max.get() as f32 {
                return false;
            }
            t_shape_hit = t1;
            // compute sphere hit position and $\phi$
            p_hit = ray.position(t_shape_hit.v);

            // refine sphere intersection point
            p_hit *= self.radius / pnt3_distancef(&p_hit, &Point3f::default());
            if p_hit.x == 0.0 && p_hit.y == 0.0 {
                p_hit.x = 1e-5_f32 * self.radius;
            }
            phi = p_hit.y.atan2(p_hit.x);
            if phi < 0.0 {
                phi += 2.0_f32 * PI;
            }
            if (self.z_min > -self.radius && p_hit.z < self.z_min)
                || (self.z_max < self.radius && p_hit.z > self.z_max)
                || phi > self.phi_max
            {
                return false;
            }
        }
        // find parametric representation of sphere hit
        let u: Float = phi / self.phi_max;
        let theta: Float = clamp_t(p_hit.z / self.radius, -1.0, 1.0).acos();
        let v: Float = (theta - self.theta_min) / (self.theta_max - self.theta_min);
        // compute sphere $\dpdu$ and $\dpdv$
        let z_radius: Float = (p_hit.x * p_hit.x + p_hit.y * p_hit.y).sqrt();
        let inv_z_radius: Float = 1.0 / z_radius;
        let cos_phi: Float = p_hit.x * inv_z_radius;
        let sin_phi: Float = p_hit.y * inv_z_radius;
        let dpdu: Vector3f = Vector3f {
            x: -self.phi_max * p_hit.y,
            y: self.phi_max * p_hit.x,
            z: 0.0,
        };
        let dpdv: Vector3f = Vector3f {
            x: p_hit.z * cos_phi,
            y: p_hit.z * sin_phi,
            z: -self.radius * theta.sin(),
        } * (self.theta_max - self.theta_min);
        // compute sphere $\dndu$ and $\dndv$
        let d2_p_duu: Vector3f = Vector3f {
            x: p_hit.x,
            y: p_hit.y,
            z: 0.0,
        } * -self.phi_max
            * self.phi_max;
        let d2_p_duv: Vector3f = Vector3f {
            x: -sin_phi,
            y: cos_phi,
            z: 0.0,
        } * (self.theta_max - self.theta_min)
            * p_hit.z
            * self.phi_max;
        let d2_p_dvv: Vector3f = Vector3f {
            x: p_hit.x,
            y: p_hit.y,
            z: p_hit.z,
        } * -(self.theta_max - self.theta_min)
            * (self.theta_max - self.theta_min);
        // compute coefficients for fundamental forms
        let ec: Float = vec3_dot_vec3f(&dpdu, &dpdu);
        let fc: Float = vec3_dot_vec3f(&dpdu, &dpdv);
        let gc: Float = vec3_dot_vec3f(&dpdv, &dpdv);
        let nc: Vector3f = vec3_cross_vec3(&dpdu, &dpdv).normalize();
        let el: Float = vec3_dot_vec3f(&nc, &d2_p_duu);
        let fl: Float = vec3_dot_vec3f(&nc, &d2_p_duv);
        let gl: Float = vec3_dot_vec3f(&nc, &d2_p_dvv);
        // compute $\dndu$ and $\dndv$ from fundamental form coefficients
        let inv_egf2: Float = 1.0 / (ec * gc - fc * fc);
        let dndu = dpdu * (fl * fc - el * gc) * inv_egf2 + dpdv * (el * fc - fl * ec) * inv_egf2;
        let dndu = Normal3f {
            x: dndu.x,
            y: dndu.y,
            z: dndu.z,
        };
        let dndv = dpdu * (gl * fc - fl * gc) * inv_egf2 + dpdv * (fl * fc - gl * ec) * inv_egf2;
        let dndv = Normal3f {
            x: dndv.x,
            y: dndv.y,
            z: dndv.z,
        };
        // compute error bounds for sphere intersection
        let p_error: Vector3f = Vector3f {
            x: p_hit.x,
            y: p_hit.y,
            z: p_hit.z,
        }
        .abs()
            * gamma(5_i32);
        // initialize _SurfaceInteraction_ from parametric information
        let uv_hit: Point2f = Point2f { x: u, y: v };
        let wo: Vector3f = -ray.d;
        *isect = SurfaceInteraction::new(
            &p_hit, &p_error, uv_hit, &wo, &dpdu, &dpdv, &dndu, &dndv, ray.time, None,
        );
        self.object_to_world.transform_surface_interaction(isect);
        // if let Some(ref shape) = si.shape {
        //     isect.shape = Some(shape.clone());
        // }
        // if let Some(primitive) = si.primitive {
        //     isect.primitive = Some(primitive.clone());
        // }
        *t_hit = t_shape_hit.v as Float;
        true
    }
    pub fn intersect_p(&self, r: &Ray) -> bool {
        // transform _Ray_ to object space
        let mut o_err: Vector3f = Vector3f::default();
        let mut d_err: Vector3f = Vector3f::default();
        let ray: Ray = self
            .world_to_object
            .transform_ray_with_error(r, &mut o_err, &mut d_err);

        // compute quadratic sphere coefficients

        // initialize _EFloat_ ray coordinate values
        let ox = EFloat::new(ray.o.x as f32, o_err.x as f32);
        let oy = EFloat::new(ray.o.y as f32, o_err.y as f32);
        let oz = EFloat::new(ray.o.z as f32, o_err.z as f32);
        let dx = EFloat::new(ray.d.x as f32, d_err.x as f32);
        let dy = EFloat::new(ray.d.y as f32, d_err.y as f32);
        let dz = EFloat::new(ray.d.z as f32, d_err.z as f32);
        let a: EFloat = dx * dx + dy * dy + dz * dz;
        let b: EFloat = (dx * ox + dy * oy + dz * oz) * 2.0f32;
        let c: EFloat = ox * ox + oy * oy + oz * oz
            - EFloat::new(self.radius as f32, 0.0) * EFloat::new(self.radius as f32, 0.0);

        // solve quadratic equation for _t_ values
        let mut t0: EFloat = EFloat::default();
        let mut t1: EFloat = EFloat::default();
        if !quadratic_efloat(a, b, c, &mut t0, &mut t1) {
            return false;
        }
        // check quadric shape _t0_ and _t1_ for nearest intersection
        if t0.upper_bound() > ray.t_max.get() as f32 || t1.lower_bound() <= 0.0f32 {
            return false;
        }
        let mut t_shape_hit: EFloat = t0;
        if t_shape_hit.lower_bound() <= 0.0f32 {
            t_shape_hit = t1;
            if t_shape_hit.upper_bound() > ray.t_max.get() as f32 {
                return false;
            }
        }
        // compute sphere hit position and $\phi$
        let mut p_hit: Point3f = ray.position(t_shape_hit.v);
        // refine sphere intersection point
        p_hit *= self.radius / pnt3_distancef(&p_hit, &Point3f::default());
        if p_hit.x == 0.0 && p_hit.y == 0.0 {
            p_hit.x = 1e-5_f32 * self.radius;
        }
        let mut phi: Float = p_hit.y.atan2(p_hit.x);
        if phi < 0.0 {
            phi += 2.0_f32 * PI;
        }
        // test sphere intersection against clipping parameters
        if (self.z_min > -self.radius && p_hit.z < self.z_min)
            || (self.z_max < self.radius && p_hit.z > self.z_max)
            || phi > self.phi_max
        {
            if t_shape_hit == t1 {
                return false;
            }
            if t1.upper_bound() > ray.t_max.get() as f32 {
                return false;
            }
            t_shape_hit = t1;
            // compute sphere hit position and $\phi$
            p_hit = ray.position(t_shape_hit.v);

            // refine sphere intersection point
            p_hit *= self.radius / pnt3_distancef(&p_hit, &Point3f::default());
            if p_hit.x == 0.0 && p_hit.y == 0.0 {
                p_hit.x = 1e-5_f32 * self.radius;
            }
            phi = p_hit.y.atan2(p_hit.x);
            if phi < 0.0 {
                phi += 2.0_f32 * PI;
            }
            if (self.z_min > -self.radius && p_hit.z < self.z_min)
                || (self.z_max < self.radius && p_hit.z > self.z_max)
                || phi > self.phi_max
            {
                return false;
            }
        }
        true
    }
    pub fn get_reverse_orientation(&self) -> bool {
        self.reverse_orientation
    }
    pub fn get_transform_swaps_handedness(&self) -> bool {
        self.transform_swaps_handedness
    }
    pub fn get_object_to_world(&self) -> Transform {
        self.object_to_world
    }
    pub fn area(&self) -> Float {
        self.phi_max * self.radius * (self.z_max - self.z_min)
    }
    pub fn sample(&self, u: Point2f, pdf: &mut Float) -> InteractionCommon {
        let mut p_obj: Point3f = Point3f::default() + uniform_sample_sphere(u) * self.radius;
        let mut it: InteractionCommon = InteractionCommon::default();
        it.n = self
            .object_to_world
            .transform_normal(&Normal3f {
                x: p_obj.x,
                y: p_obj.y,
                z: p_obj.z,
            })
            .normalize();
        if self.reverse_orientation {
            it.n *= -1.0 as Float;
        }
        // reproject _p_obj_ to sphere surface and compute _p_obj_error_
        p_obj *= self.radius / pnt3_distancef(&p_obj, &Point3f::default());
        let p_obj_error: Vector3f = Vector3f::from(p_obj).abs() * gamma(5_i32);
        it.p = self.object_to_world.transform_point_with_abs_error(
            &p_obj,
            &p_obj_error,
            &mut it.p_error,
        );
        *pdf = 1.0 as Float / self.area();
        it
    }
    pub fn sample_with_ref_point(
        &self,
        iref: &InteractionCommon,
        u: Point2f,
        pdf: &mut Float,
    ) -> InteractionCommon {
        let p_center: Point3f = self.object_to_world.transform_point(&Point3f::default());
        // sample uniformly on sphere if $\pt{}$ is inside it
        let p_origin: Point3f =
            pnt3_offset_ray_origin(&iref.p, &iref.p_error, &iref.n, &(p_center - iref.p));
        if pnt3_distance_squaredf(&p_origin, &p_center) <= self.radius * self.radius {
            let intr: InteractionCommon = self.sample(u, pdf);
            let mut wi: Vector3f = intr.p - iref.p;
            if wi.length_squared() == 0.0 as Float {
                *pdf = 0.0 as Float;
            } else {
                // convert from area measure returned by Sample() call
                // above to solid angle measure.
                wi = wi.normalize();
                *pdf *= pnt3_distance_squaredf(&iref.p, &intr.p) / nrm_abs_dot_vec3f(&intr.n, &-wi);
            }
            if (*pdf).is_infinite() {
                *pdf = 0.0 as Float;
            }
            return intr;
        }

        // compute coordinate system for sphere sampling
        let wc: Vector3f = (p_center - iref.p).normalize();
        let mut wc_x: Vector3f = Vector3f::default();
        let mut wc_y: Vector3f = Vector3f::default();
        vec3_coordinate_system(&wc, &mut wc_x, &mut wc_y);
        // sample sphere uniformly inside subtended cone

        // compute $\theta$ and $\phi$ values for sample in cone
        let sin_theta_max2: Float =
            self.radius * self.radius / pnt3_distance_squaredf(&iref.p, &p_center);
        let cos_theta_max: Float = (0.0 as Float).max(1.0 as Float - sin_theta_max2).sqrt();
        let cos_theta: Float = (1.0 as Float - u[XYEnum::X]) + u[XYEnum::X] * cos_theta_max;
        let sin_theta: Float = (0.0 as Float)
            .max(1.0 as Float - cos_theta * cos_theta)
            .sqrt();
        let phi: Float = u[XYEnum::Y] * 2.0 as Float * PI;
        // compute angle $\alpha$ from center of sphere to sampled point on surface
        let dc: Float = pnt3_distancef(&iref.p, &p_center);
        let ds: Float = dc * cos_theta
            - (0.0 as Float)
                .max(self.radius * self.radius - dc * dc * sin_theta * sin_theta)
                .sqrt();
        let cos_alpha: Float =
            (dc * dc + self.radius * self.radius - ds * ds) / (2.0 as Float * dc * self.radius);
        let sin_alpha: Float = (0.0 as Float)
            .max(1.0 as Float - cos_alpha * cos_alpha)
            .sqrt();
        // compute surface normal and sampled point on sphere
        let n_world: Vector3f =
            spherical_direction_vec3(sin_alpha, cos_alpha, phi, &(-wc_x), &(-wc_y), &(-wc));
        let p_world: Point3f = p_center
            + Point3f {
                x: n_world.x,
                y: n_world.y,
                z: n_world.z,
            } * self.radius;
        // return _Interaction_ for sampled point on sphere
        let mut it: InteractionCommon = InteractionCommon::default();
        it.p = p_world;
        it.p_error = Vector3f::from(p_world).abs() * gamma(5_i32);
        it.n = Normal3f::from(n_world);
        if self.reverse_orientation {
            it.n *= -1.0 as Float;
        }
        // uniform cone PDF.
        *pdf = 1.0 as Float / (2.0 as Float * PI * (1.0 as Float - cos_theta_max));
        it
    }
    pub fn pdf_with_ref_point(&self, iref: &dyn Interaction, wi: &Vector3f) -> Float {
        let p_center: Point3f = self.object_to_world.transform_point(&Point3f::default());
        // return uniform PDF if point is inside sphere
        let p_origin: Point3f = pnt3_offset_ray_origin(
            &iref.get_p(),
            &iref.get_p_error(),
            &iref.get_n(),
            &(p_center - *iref.get_p()),
        );
        if pnt3_distance_squaredf(&p_origin, &p_center) <= self.radius * self.radius {
            // return Shape::Pdf(ref, wi);

            // intersect sample ray with area light geometry
            let ray: Ray = iref.spawn_ray(wi);
            // ignore any alpha textures used for trimming the shape when
            // performing this intersection. Hack for the "San Miguel"
            // scene, where this is used to make an invisible area light.
            let mut t_hit: Float = 0.0;
            let mut isect_light: SurfaceInteraction = SurfaceInteraction::default();
            if self.intersect(&ray, &mut t_hit, &mut isect_light) {
                // convert light sample weight to solid angle measure
                let mut pdf: Float = pnt3_distance_squaredf(&iref.get_p(), &isect_light.common.p)
                    / (nrm_abs_dot_vec3f(&isect_light.common.n, &-(*wi)) * self.area());
                if pdf.is_infinite() {
                    pdf = 0.0 as Float;
                }
                return pdf;
            } else {
                return 0.0 as Float;
            }
        }
        // compute general sphere PDF
        let sin_theta_max2: Float =
            self.radius * self.radius / pnt3_distance_squaredf(&iref.get_p(), &p_center);
        let cos_theta_max: Float = (0.0 as Float).max(1.0 as Float - sin_theta_max2).sqrt();
        uniform_cone_pdf(cos_theta_max)
    }
}
