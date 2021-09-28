// std
use std::ops::DerefMut;
use std::sync::Arc;
// pbrt
use crate::core::camera::Camera;
use crate::core::film::Film;
use crate::core::geometry::{Bounds2f, Bounds2i, Point2f, Point2i};
use crate::core::integrator::compute_light_power_distribution;
use crate::core::pbrt::erf_inv;
use crate::core::pbrt::SQRT_2;
use crate::core::pbrt::{Float, Spectrum};
use crate::core::rng::Rng;
use crate::core::sampler::Sampler;
use crate::core::sampling::Distribution1D;
use crate::core::scene::Scene;
use crate::integrators::bdpt::Vertex;
use crate::integrators::bdpt::{connect_bdpt, generate_camera_subpath, generate_light_subpath};
// others

pub const CAMERA_STREAM_INDEX: u8 = 0;
pub const LIGHT_STREAM_INDEX: u8 = 1;
pub const CONNECTION_STREAM_INDEX: u8 = 2;
pub const N_SAMPLE_STREAMS: u8 = 3;

#[derive(Debug, Default, Copy, Clone)]
pub struct PrimarySample {
    pub value: Float,
    pub last_modification_iteration: i64,
    pub value_backup: Float,
    pub modify_backup: i64,
}

impl PrimarySample {
    pub fn backup(&mut self) {
        self.value_backup = self.value;
        self.modify_backup = self.last_modification_iteration;
    }
    pub fn restore(&mut self) {
        self.value = self.value_backup;
        self.last_modification_iteration = self.modify_backup;
    }
}

#[derive(Clone)]
pub struct MLTSampler {
    pub samples_per_pixel: i64,
    pub rng: Rng,
    pub sigma: Float,
    pub large_step_probability: Float,
    pub stream_count: i32,
    pub x: Vec<PrimarySample>,
    pub current_iteration: i64,
    pub large_step: bool,
    pub last_large_step_iteration: i64,
    pub stream_index: i32,
    pub sample_index: i32,
    // inherited from class Sampler (see sampler.h)
    pub current_pixel: Point2i,
    pub current_pixel_sample_index: i64,
    pub samples_1d_array_sizes: Vec<i32>,
    pub samples_2d_array_sizes: Vec<i32>,
    pub sample_array_1d: Vec<Vec<Float>>,
    pub sample_array_2d: Vec<Vec<Point2f>>,
    pub array_1d_offset: usize,
    pub array_2d_offset: usize,
}

impl MLTSampler {
    pub fn new(
        mutations_per_pixel: i64,
        rng_sequence_index: u64,
        sigma: Float,
        large_step_probability: Float,
        stream_count: i32,
    ) -> Self {
        let mut rng: Rng = Rng::default();
        rng.set_sequence(rng_sequence_index);
        MLTSampler {
            samples_per_pixel: mutations_per_pixel,
            rng,
            sigma,
            large_step_probability,
            stream_count,
            x: Vec::new(),
            current_iteration: 0_i64,
            large_step: true,
            last_large_step_iteration: 0_i64,
            stream_index: 0_i32,
            sample_index: 0_i32,
            current_pixel: Point2i::default(),
            current_pixel_sample_index: 0_i64,
            samples_1d_array_sizes: Vec::new(),
            samples_2d_array_sizes: Vec::new(),
            sample_array_1d: Vec::new(),
            sample_array_2d: Vec::new(),
            array_1d_offset: 0_usize,
            array_2d_offset: 0_usize,
        }
    }
    pub fn clone_with_seed(&self, _seed: u64) -> Box<Sampler> {
        let mlt_sampler = MLTSampler {
            samples_per_pixel: self.samples_per_pixel,
            rng: self.rng,
            sigma: self.sigma,
            large_step_probability: self.large_step_probability,
            stream_count: self.stream_count,
            x: self.x.clone(),
            current_iteration: self.current_iteration,
            large_step: self.large_step,
            last_large_step_iteration: self.last_large_step_iteration,
            stream_index: self.stream_index,
            sample_index: self.sample_index,
            current_pixel: self.current_pixel,
            current_pixel_sample_index: self.current_pixel_sample_index,
            samples_1d_array_sizes: self.samples_1d_array_sizes.to_vec(),
            samples_2d_array_sizes: self.samples_2d_array_sizes.to_vec(),
            sample_array_1d: self.sample_array_1d.to_vec(),
            sample_array_2d: self.sample_array_2d.to_vec(),
            array_1d_offset: self.array_1d_offset,
            array_2d_offset: self.array_2d_offset,
        };
        let sampler = Sampler::MLT(mlt_sampler);
        Box::new(sampler)
    }
    pub fn start_iteration(&mut self) {
        self.current_iteration += 1;
        self.large_step = self.rng.uniform_float() < self.large_step_probability;
    }
    pub fn accept(&mut self) {
        if self.large_step {
            self.last_large_step_iteration = self.current_iteration;
        }
    }
    pub fn reject(&mut self) {
        for i in 0..self.x.len() {
            if let Some(xi) = self.x.get_mut(i as usize) {
                if xi.last_modification_iteration == self.current_iteration {
                    xi.restore();
                }
            } else {
                panic!("self.x.get_mut({:?}) failed", i);
            }
        }
        self.current_iteration -= 1;
    }
    pub fn start_stream(&mut self, index: i32) {
        assert!(index < self.stream_count);
        self.stream_index = index;
        self.sample_index = 0;
    }
    pub fn get_next_index(&mut self) -> i32 {
        let ret = self.stream_index + self.stream_count * self.sample_index;
        self.sample_index += 1;
        ret
    }
    // private
    fn ensure_ready(&mut self, index: i32) {
        // enlarge _MLTSampler::x_ if necessary and get current $\VEC{X}_i$
        if index as usize >= self.x.len() {
            self.x
                .resize((index + 1) as usize, PrimarySample::default());
        }
        if let Some(xi) = self.x.get_mut(index as usize) {
            // reset $\VEC{X}_i$ if a large step took place in the meantime
            if xi.last_modification_iteration < self.last_large_step_iteration {
                xi.value = self.rng.uniform_float();
                xi.last_modification_iteration = self.last_large_step_iteration;
            }
            // apply remaining sequence of mutations to _sample_
            xi.backup();
            if self.large_step {
                xi.value = self.rng.uniform_float();
            } else {
                let n_small: i64 = self.current_iteration - xi.last_modification_iteration;
                // apply _n_small_ small step mutations

                // sample the standard normal distribution $N(0, 1)$
                let normal_sample: Float =
                    SQRT_2 * erf_inv(2.0 as Float * self.rng.uniform_float() - 1.0 as Float);
                // compute the effective standard deviation and apply perturbation to
                // $\VEC{X}_i$
                let eff_sigma: Float = self.sigma * (n_small as Float).sqrt();
                xi.value += normal_sample * eff_sigma;
                xi.value -= xi.value.floor();
            }
            xi.last_modification_iteration = self.current_iteration;
        } else {
            panic!("self.x.get_mut({:?}) failed", index);
        }
    }
    // Sampler
    pub fn start_pixel(&mut self, p: Point2i) {
        // Sampler::StartPixel(p);
        self.current_pixel = p;
        self.current_pixel_sample_index = 0_i64;
        self.array_1d_offset = 0_usize;
        self.array_2d_offset = 0_usize;
    }
    pub fn get_1d(&mut self) -> Float {
        // TODO: ProfilePhase _(Prof::GetSample);
        let index: i32 = self.get_next_index();
        self.ensure_ready(index);
        self.x[index as usize].value
    }
    pub fn get_2d(&mut self) -> Point2f {
        // C++: call x first
        let x: Float = self.get_1d();
        let y: Float = self.get_1d();
        Point2f { x, y }
    }
    pub fn get_2d_sample(&self, array_idx: usize, idx: usize) -> Point2f {
        self.sample_array_2d[array_idx][idx]
    }
    pub fn reseed(&mut self, seed: u64) {
        self.rng.set_sequence(seed);
    }
    pub fn request_2d_array(&mut self, n: i32) {
        assert_eq!(self.round_count(n), n);
        self.samples_2d_array_sizes.push(n);
        let size: usize = (n * self.samples_per_pixel as i32) as usize;
        let additional_points: Vec<Point2f> = vec![Point2f::default(); size];
        self.sample_array_2d.push(additional_points);
    }
    pub fn round_count(&self, count: i32) -> i32 {
        count
    }
    pub fn get_2d_array(&mut self, n: i32) -> Option<&[Point2f]> {
        if self.array_2d_offset == self.sample_array_2d.len() {
            return None;
        }
        assert_eq!(self.samples_2d_array_sizes[self.array_2d_offset], n);
        assert!(
            self.current_pixel_sample_index < self.samples_per_pixel,
            "self.current_pixel_sample_index ({}) < self.samples_per_pixel ({})",
            self.current_pixel_sample_index,
            self.samples_per_pixel
        );
        let start: usize = (self.current_pixel_sample_index * n as i64) as usize;
        let end: usize = start + n as usize;
        self.array_2d_offset += 1;
        Some(&self.sample_array_2d[self.array_2d_offset - 1][start..end])
    }
    pub fn get_2d_array_idxs(&mut self, n: i32) -> (bool, usize, usize) {
        if self.array_2d_offset == self.sample_array_2d.len() {
            return (true, 0_usize, 0_usize);
        }
        assert_eq!(self.samples_2d_array_sizes[self.array_2d_offset], n);
        assert!(
            self.current_pixel_sample_index < self.samples_per_pixel,
            "self.current_pixel_sample_index ({}) < self.samples_per_pixel ({})",
            self.current_pixel_sample_index,
            self.samples_per_pixel
        );
        let start: usize = (self.current_pixel_sample_index * n as i64) as usize;
        let idx: usize = self.array_2d_offset;
        self.array_2d_offset += 1;
        (false, idx, start)
    }
    pub fn start_next_sample(&mut self) -> bool {
        // reset array offsets for next pixel sample
        self.array_1d_offset = 0_usize;
        self.array_2d_offset = 0_usize;
        self.current_pixel_sample_index += 1_i64;
        self.current_pixel_sample_index < self.samples_per_pixel
    }
    pub fn get_current_pixel(&self) -> Point2i {
        self.current_pixel
    }
    pub fn get_current_sample_number(&self) -> i64 {
        self.current_pixel_sample_index
    }
    pub fn get_samples_per_pixel(&self) -> i64 {
        self.samples_per_pixel
    }
}

/// Metropolis Light Transport
pub struct MLTIntegrator {
    pub camera: Arc<Camera>,
    pub max_depth: u32,
    pub n_bootstrap: u32,
    pub n_chains: u32,
    pub mutations_per_pixel: u32,
    pub sigma: Float,
    pub large_step_probability: Float,
}

impl MLTIntegrator {
    pub fn new(
        camera: Arc<Camera>,
        max_depth: u32,
        n_bootstrap: u32,
        n_chains: u32,
        mutations_per_pixel: u32,
        sigma: Float,
        large_step_probability: Float,
    ) -> Self {
        MLTIntegrator {
            camera,
            max_depth,
            n_bootstrap,
            n_chains,
            mutations_per_pixel,
            sigma,
            large_step_probability,
        }
    }
    pub fn l(
        &self,
        scene: &Scene,
        light_distr: Arc<Distribution1D>,
        sampler: &mut Sampler,
        depth: u32,
        p_raster: &mut Point2f,
    ) -> Spectrum {
        match sampler {
            Sampler::MLT(mlt_sampler) => mlt_sampler.start_stream(CAMERA_STREAM_INDEX as i32),
            _ => panic!("MLTSampler needed."),
        }
        // determine the number of available strategies and pick a specific one
        let s: u32;
        let t: u32;
        let n_strategies: u32;
        if depth == 0_u32 {
            n_strategies = 1;
            s = 0;
            t = 2;
        } else {
            n_strategies = depth + 2;
            s = ((sampler.get_1d() * n_strategies as Float) as u32).min(n_strategies - 1);
            t = n_strategies - s;
        }
        // generate a camera subpath with exactly _t_ vertices
        let mut camera_vertices: Vec<Vertex> = Vec::with_capacity(t as usize);
        let film = self.camera.get_film();
        let sample_bounds: Bounds2i = film.get_sample_bounds();
        let sample_bounds_f: Bounds2f = Bounds2f {
            p_min: Point2f {
                x: sample_bounds.p_min.x as Float,
                y: sample_bounds.p_min.y as Float,
            },
            p_max: Point2f {
                x: sample_bounds.p_max.x as Float,
                y: sample_bounds.p_max.y as Float,
            },
        };
        *p_raster = sample_bounds_f.lerp(sampler.get_2d());
        let n_camera;
        let time;
        {
            let (n_camera_new, _p_new, time_new) = generate_camera_subpath(
                scene,
                sampler,
                t,
                &self.camera,
                *p_raster,
                &mut camera_vertices,
            );
            n_camera = n_camera_new;
            time = time_new;
        }
        if n_camera != t as usize {
            return Spectrum::default();
        }
        // generate a light subpath with exactly _s_ vertices
        match sampler {
            Sampler::MLT(mlt_sampler) => mlt_sampler.start_stream(LIGHT_STREAM_INDEX as i32),
            _ => panic!("MLTSampler needed."),
        }
        let mut light_vertices: Vec<Vertex> = Vec::with_capacity(s as usize);
        let n_light;
        {
            n_light = generate_light_subpath(
                scene,
                sampler,
                s,
                time,
                light_distr.clone(),
                // light_to_index,
                &mut light_vertices,
            );
        }
        if n_light != s as usize {
            return Spectrum::default();
        }
        // execute connection strategy and return the radiance estimate
        match sampler {
            Sampler::MLT(mlt_sampler) => mlt_sampler.start_stream(CONNECTION_STREAM_INDEX as i32),
            _ => panic!("MLTSampler needed."),
        }
        connect_bdpt(
            scene,
            &light_vertices,
            &camera_vertices,
            s as usize,
            t as usize,
            light_distr,
            // light_to_index,
            &self.camera,
            sampler,
            p_raster,
            None,
        ) * (n_strategies as Float)
    }
    pub fn render(&self, scene: &Scene, num_threads: u8) {
        let mut num_cores: usize;
        let num_cores_init = if num_threads == 0_u8 {
            1
        } else {
            num_threads as usize
        };
        if let Some(light_distr) = compute_light_power_distribution(scene) {
            println!("Generating bootstrap paths ...");
            // generate bootstrap samples and compute normalization constant $b$
            num_cores = 1; // TMP: disable multi-threading
            let n_bootstrap_samples: u32 = self.n_bootstrap * (self.max_depth + 1);
            let mut bootstrap_weights: Vec<Float> =
                vec![0.0 as Float; n_bootstrap_samples as usize];
            if !scene.lights.is_empty() {
                // TODO: ProgressReporter progress(nBootstrap / 256, "Generating bootstrap paths");
                // let chunk_size: u32 = clamp_t(integrator.n_bootstrap / 128, 1, 8192);
                let chunk_size: usize = (n_bootstrap_samples / num_cores as u32) as usize;
                {
                    let bands: Vec<&mut [Float]> =
                        bootstrap_weights.chunks_mut(chunk_size).collect();
                    let integrator = &self;
                    let light_distr = &light_distr;
                    // crossbeam::scope(|scope| {
                    //     let (band_tx, band_rx) = crossbeam_channel::bounded(num_cores);
                    // spawn worker threads
                    for (b, band) in bands.into_iter().enumerate() {
                        // let band_tx = band_tx.clone();
                        // scope.spawn(move |_| {
                        for (w, weight) in band.iter_mut().enumerate() {
                            let rng_index: u64 = ((b * chunk_size) + w) as u64;
                            let depth: u32 = (rng_index % (integrator.max_depth + 1) as u64) as u32;
                            let mut sampler: Box<Sampler> =
                                Box::new(Sampler::MLT(MLTSampler::new(
                                    integrator.mutations_per_pixel as i64,
                                    rng_index,
                                    integrator.sigma,
                                    integrator.large_step_probability,
                                    N_SAMPLE_STREAMS as i32,
                                )));
                            let mut p_raster: Point2f = Point2f::default();
                            *weight = integrator
                                .l(
                                    scene,
                                    light_distr.clone(),
                                    &mut sampler,
                                    depth,
                                    &mut p_raster,
                                )
                                .y();
                        }
                    }
                    //                            });
                    // send progress through the channel to main thread
                }
                //     })
                //     .unwrap();
                // }
            }
            let bootstrap: Distribution1D = Distribution1D::new(bootstrap_weights);
            let b: Float = bootstrap.func_int * (self.max_depth + 1) as Float;
            // run _n_chains_ Markov chains in parallel
            num_cores = num_cores_init; // TMP: re-enable multi-threading
            let film: Arc<Film> = self.get_camera().get_film();
            let n_total_mutations: u64 =
                self.mutations_per_pixel as u64 * film.get_sample_bounds().area() as u64;
            if !scene.lights.is_empty() {
                // TODO: let progress_frequency = 32768;
                // TODO: ProgressReporter progress(nTotalMutations / progressFrequency,
                //                           "Rendering");
                // use parallel iterator (par_iter_with) from rayon crate
                //                let (sender, receiver) = crossbeam_channel::bounded(num_cores);
                let n_chains = self.n_chains;
                // for i in 0..n_chains {
                let ivec: Vec<u32> = (0..n_chains).collect();
                for (s, &i) in ivec.iter().enumerate() {
                    //                ivec.par_iter().for_each_with(sender, |s, &i| {
                    //                  s.send(i).unwrap_or_else(|_| panic!("Failed to send chain"));
                    let n_chain_mutations: u64 = ((i as u64 + 1) * n_total_mutations
                        / n_chains as u64)
                        .min(n_total_mutations)
                        - i as u64 * n_total_mutations / n_chains as u64;
                    // select initial state from the set of bootstrap samples
                    let mut rng: Rng = Rng::default();
                    rng.set_sequence(i as u64);
                    let bootstrap_index: usize =
                        bootstrap.sample_discrete(rng.uniform_float(), None);
                    let depth: u32 = bootstrap_index as u32 % (self.max_depth as u32 + 1);
                    // initialize local variables for selected state
                    let mut sampler: Box<Sampler> = Box::new(Sampler::MLT(MLTSampler::new(
                        self.mutations_per_pixel as i64,
                        bootstrap_index as u64,
                        self.sigma,
                        self.large_step_probability,
                        N_SAMPLE_STREAMS as i32,
                    )));
                    let mut p_current: Point2f = Point2f::default();
                    let mut l_current: Spectrum = self.l(
                        scene,
                        light_distr.clone(),
                        &mut sampler,
                        depth,
                        &mut p_current,
                    );
                    // run the Markov chain for _n_chain_mutations_ steps
                    for _j in 0..n_chain_mutations {
                        match sampler.deref_mut() {
                            Sampler::MLT(mlt_sampler) => mlt_sampler.start_iteration(),
                            _ => panic!("MLTSampler needed."),
                        }
                        let mut p_proposed: Point2f = Point2f::default();
                        let l_proposed: Spectrum = self.l(
                            scene,
                            light_distr.clone(),
                            &mut sampler,
                            depth,
                            &mut p_proposed,
                        );
                        // compute acceptance probability for proposed sample
                        let accept: Float = (1.0 as Float).min(l_proposed.y() / l_current.y());
                        // splat both current and proposed samples to _film_
                        if accept > 0.0 as Float {
                            film.add_splat(p_proposed, &(l_proposed * accept / l_proposed.y()));
                        }
                        film.add_splat(
                            p_current,
                            &(l_current * (1.0 as Float - accept) / l_current.y()),
                        );
                        // accept or reject the proposal
                        if rng.uniform_float() < accept {
                            p_current = p_proposed;
                            l_current = l_proposed;
                            match sampler.deref_mut() {
                                Sampler::MLT(mlt_sampler) => mlt_sampler.accept(),
                                _ => panic!("MLTSampler needed."),
                            }
                        // TODO: ++acceptedMutations;
                        } else {
                            match sampler.deref_mut() {
                                Sampler::MLT(mlt_sampler) => mlt_sampler.reject(),
                                _ => panic!("MLTSampler needed."),
                            }
                        }
                        // TODO: ++totalMutations;
                        // if (i * n_total_mutations / n_chains + j) % progress_frequency == 0 {
                        //     progress.update();
                        // }
                        // TODO: arena.Reset();
                    }
                }
            }
            // Store final image computed with MLT
            film.write_image(b / self.mutations_per_pixel as Float);
        }
    }
    pub fn get_camera(&self) -> Arc<Camera> {
        self.camera.clone()
    }
}
