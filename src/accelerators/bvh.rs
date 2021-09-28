// std
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// others
// use time::PreciseTime;
use typed_arena::Arena;
// pbrt
use crate::core::geometry::{bnd3_union_bnd3f, bnd3_union_pnt3f};
use crate::core::geometry::{Bounds3f, Point3f, Ray, Vector3f, XYZEnum};
use crate::core::interaction::SurfaceInteraction;
use crate::core::light::Light;
use crate::core::material::Material;
use crate::core::paramset::ParamSet;
use crate::core::pbrt::Float;
use crate::core::primitive::Primitive;

// see bvh.h

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SplitMethod {
    SAH,
    HLBVH,
    Middle,
    EqualCounts,
}

#[derive(Debug, Default, Copy, Clone)]
pub struct BVHPrimitiveInfo {
    primitive_number: usize,
    bounds: Bounds3f,
    centroid: Point3f,
}

impl BVHPrimitiveInfo {
    pub fn new(primitive_number: usize, bounds: Bounds3f) -> Self {
        BVHPrimitiveInfo {
            primitive_number,
            bounds,
            centroid: bounds.p_min * 0.5 + bounds.p_max * 0.5,
        }
    }
}

#[derive(Debug)]
pub struct BVHBuildNode<'a> {
    pub bounds: Bounds3f,
    pub child1: Option<&'a BVHBuildNode<'a>>,
    pub child2: Option<&'a BVHBuildNode<'a>>,
    pub split_axis: u8,
    pub first_prim_offset: usize,
    pub n_primitives: usize,
}

impl<'a> Default for BVHBuildNode<'a> {
    fn default() -> Self {
        BVHBuildNode {
            bounds: Bounds3f::default(),
            child1: None,
            child2: None,
            split_axis: 0_u8,
            first_prim_offset: 0_usize,
            n_primitives: 0_usize,
        }
    }
}

impl<'a> BVHBuildNode<'a> {
    pub fn init_leaf(&mut self, first: usize, n: usize, b: &Bounds3f) {
        self.first_prim_offset = first;
        self.n_primitives = n;
        self.bounds = *b;
        self.child1 = None;
        self.child2 = None;
    }
    pub fn init_interior(&mut self, axis: u8, c0: &'a BVHBuildNode<'a>, c1: &'a BVHBuildNode<'a>) {
        self.n_primitives = 0;
        self.bounds = bnd3_union_bnd3f(&c0.bounds, &c1.bounds);
        self.child1 = Some(c0);
        self.child2 = Some(c1);
        self.split_axis = axis;
    }
}

#[derive(Debug, Copy, Clone)]
struct BucketInfo {
    count: usize,
    bounds: Bounds3f,
}

impl Default for BucketInfo {
    fn default() -> Self {
        BucketInfo {
            count: 0_usize,
            bounds: Bounds3f::default(),
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct LinearBVHNode {
    bounds: Bounds3f,
    // in C++ a union { int primitivesOffset;     // leaf
    //                  int secondChildOffset; }; // interior
    offset: i32,
    n_primitives: u16,
    axis: u8,
    pad: u8,
}

// BVHAccel -> Aggregate -> Primitive
#[derive(Serialize, Deserialize)]
pub struct BVHAccel {
    max_prims_in_node: usize,
    split_method: SplitMethod,
    pub primitives: Vec<Arc<Primitive>>,
    pub nodes: Vec<LinearBVHNode>,
}

impl BVHAccel {
    pub fn new(
        p: Vec<Arc<Primitive>>,
        max_prims_in_node: usize,
        split_method: SplitMethod,
    ) -> Self {
        let bvh = Arc::new(BVHAccel {
            max_prims_in_node: std::cmp::min(max_prims_in_node, 255),
            split_method: split_method.clone(),
            primitives: p,
            nodes: Vec::new(),
        });
        let num_prims = bvh.primitives.len();
        if num_prims == 0_usize {
            let unwrapped = Arc::try_unwrap(bvh);
            return unwrapped.ok().unwrap();
        }
        let mut primitive_info = vec![BVHPrimitiveInfo::default(); num_prims];
        for (i, item) in primitive_info.iter_mut().enumerate().take(num_prims) {
            let world_bound = bvh.primitives[i].world_bound();
            *item = BVHPrimitiveInfo::new(i, world_bound);
        }
        // TODO: if (splitMethod == SplitMethod::HLBVH)
        let arena: Arena<BVHBuildNode> = Arena::with_capacity(1024 * 1024);
        let mut total_nodes: usize = 0;
        let mut ordered_prims: Vec<Arc<Primitive>> = Vec::with_capacity(num_prims);
        // println!("BVHAccel::recursive_build(..., {}, ...)", num_prims);
        // let start = PreciseTime::now();
        let root = BVHAccel::recursive_build(
            bvh, // instead of self
            &arena,
            &mut primitive_info,
            0,
            num_prims,
            &mut total_nodes,
            &mut ordered_prims,
        );
        // let end = PreciseTime::now();
        // println!("{} seconds for building BVH ...", start.to(end));
        // flatten first
        let mut nodes = vec![LinearBVHNode::default(); total_nodes];
        let mut offset: usize = 0;
        // println!("BVHAccel::flatten_bvh_tree(...)");
        // let start = PreciseTime::now();
        BVHAccel::flatten_bvh_tree(root, &mut nodes, &mut offset);
        // let end = PreciseTime::now();
        // println!("{} seconds for flattening BVH ...", start.to(end));
        assert!(nodes.len() == total_nodes);
        // primitives.swap(orderedPrims);
        let bvh_ordered_prims = Arc::new(BVHAccel {
            max_prims_in_node: std::cmp::min(max_prims_in_node, 255),
            split_method,
            primitives: ordered_prims,
            nodes,
        });
        let unwrapped = Arc::try_unwrap(bvh_ordered_prims);
        unwrapped.ok().unwrap()
    }
    pub fn create(prims: Vec<Arc<Primitive>>, ps: &ParamSet) -> Primitive {
        let split_method_name: String = ps.find_one_string("splitmethod", String::from("sah"));
        let split_method;
        if split_method_name == "sah" {
            split_method = SplitMethod::SAH;
        } else if split_method_name == "hlbvh" {
            split_method = SplitMethod::HLBVH;
        } else if split_method_name == "middle" {
            split_method = SplitMethod::Middle;
        } else if split_method_name == "equal" {
            split_method = SplitMethod::EqualCounts;
        } else {
            println!(
                "WARNING: BVH split method \"{}\" unknown.  Using \"sah\".",
                split_method_name
            );
            split_method = SplitMethod::SAH;
        }
        let max_prims_in_node: i32 = ps.find_one_int("maxnodeprims", 4);
        Primitive::BVH(Box::new(BVHAccel::new(
            prims,
            max_prims_in_node as usize,
            split_method,
        )))
    }
    pub fn recursive_build<'a>(
        bvh: Arc<BVHAccel>,
        arena: &'a Arena<BVHBuildNode<'a>>,
        primitive_info: &mut Vec<BVHPrimitiveInfo>,
        start: usize,
        end: usize,
        total_nodes: &mut usize,
        ordered_prims: &mut Vec<Arc<Primitive>>,
    ) -> &'a BVHBuildNode<'a> {
        assert_ne!(start, end);
        let node: &mut BVHBuildNode<'a> = arena.alloc(BVHBuildNode::default());
        *total_nodes += 1_usize;
        // compute bounds of all primitives in BVH node
        let mut bounds: Bounds3f = Bounds3f::default();
        for item in primitive_info.iter().take(end).skip(start) {
            bounds = bnd3_union_bnd3f(&bounds, &item.bounds);
        }
        let n_primitives: usize = end - start;
        if n_primitives == 1 {
            // create leaf _BVHBuildNode_
            let first_prim_offset: usize = ordered_prims.len();
            for item in primitive_info.iter().take(end).skip(start) {
                let prim_num: usize = item.primitive_number;
                ordered_prims.push(bvh.primitives[prim_num].clone());
            }
            node.init_leaf(first_prim_offset, n_primitives, &bounds);
            return node;
        } else {
            // compute bound of primitive centroids, choose split dimension _dim_
            let mut centroid_bounds: Bounds3f = Bounds3f::default();
            for item in primitive_info.iter().take(end).skip(start) {
                centroid_bounds = bnd3_union_pnt3f(&centroid_bounds, &item.centroid);
            }
            let dim: u8 = centroid_bounds.maximum_extent();
            let dim_i: XYZEnum = match dim {
                0 => XYZEnum::X,
                1 => XYZEnum::Y,
                _ => XYZEnum::Z,
            };
            // partition primitives into two sets and build children
            let mut mid: usize = (start + end) / 2_usize;
            if centroid_bounds.p_max[dim_i] == centroid_bounds.p_min[dim_i] {
                // create leaf _BVHBuildNode_
                let first_prim_offset: usize = ordered_prims.len();
                for item in primitive_info.iter().take(end).skip(start) {
                    let prim_num: usize = item.primitive_number;
                    ordered_prims.push(bvh.primitives[prim_num].clone());
                }
                node.init_leaf(first_prim_offset, n_primitives, &bounds);
                return node;
            } else {
                // partition primitives based on _splitMethod_
                match bvh.split_method {
                    SplitMethod::Middle => {
                        // TODO
                    }
                    SplitMethod::EqualCounts => {
                        // TODO
                    }
                    SplitMethod::SAH | SplitMethod::HLBVH => {
                        if n_primitives <= 2 {
                            mid = (start + end) / 2;
                            if start != end - 1
                                && primitive_info[end - 1].centroid[dim_i]
                                    < primitive_info[start].centroid[dim_i]
                            {
                                primitive_info.swap(start, end - 1);
                            }
                        } else {
                            // allocate _BucketInfo_ for SAH partition buckets
                            let n_buckets: usize = 12;
                            let mut buckets: [BucketInfo; 12] = [BucketInfo::default(); 12];
                            // initialize _BucketInfo_ for SAH partition buckets
                            for item in primitive_info.iter().take(end).skip(start) {
                                let mut b: usize = (n_buckets as Float
                                    * centroid_bounds.offset(&item.centroid)[dim_i])
                                    as usize;
                                if b == n_buckets {
                                    b = n_buckets - 1;
                                }
                                // assert!(b >= 0_usize, "b >= 0");
                                assert!(b < n_buckets, "b < {}", n_buckets);
                                buckets[b].count += 1;
                                buckets[b].bounds =
                                    bnd3_union_bnd3f(&buckets[b].bounds, &item.bounds);
                            }
                            // compute costs for splitting after each bucket
                            let mut cost: [Float; 11] = [0.0; 11];
                            for (i, cost_item) in cost.iter_mut().enumerate().take(n_buckets - 1) {
                                let mut b0: Bounds3f = Bounds3f::default();
                                let mut b1: Bounds3f = Bounds3f::default();
                                let mut count0: usize = 0;
                                let mut count1: usize = 0;
                                for item in buckets.iter().take(i + 1) {
                                    b0 = bnd3_union_bnd3f(&b0, &item.bounds);
                                    count0 += item.count;
                                }
                                for item in buckets.iter().take(n_buckets).skip(i + 1) {
                                    b1 = bnd3_union_bnd3f(&b1, &item.bounds);
                                    count1 += item.count;
                                }
                                *cost_item = 1.0
                                    + (count0 as Float * b0.surface_area()
                                        + count1 as Float * b1.surface_area())
                                        / bounds.surface_area();
                            }
                            // find bucket to split at that minimizes SAH metric
                            let mut min_cost: Float = cost[0];
                            let mut min_cost_split_bucket: usize = 0;
                            for (i, item) in cost.iter().enumerate().take(n_buckets - 1) {
                                if item < &min_cost {
                                    min_cost = *item;
                                    min_cost_split_bucket = i;
                                }
                            }
                            // either create leaf or split primitives
                            // at selected SAH bucket
                            let leaf_cost: Float = n_primitives as Float;
                            if n_primitives > bvh.max_prims_in_node || min_cost < leaf_cost {
                                let (mut left, mut right): (
                                    Vec<BVHPrimitiveInfo>,
                                    Vec<BVHPrimitiveInfo>,
                                ) = primitive_info[start..end].iter().partition(|&pi| {
                                    let mut b: usize = (n_buckets as Float
                                        * centroid_bounds.offset(&pi.centroid)[dim_i])
                                        as usize;
                                    if b == n_buckets {
                                        b = n_buckets - 1;
                                    }
                                    // assert!(b >= 0_usize, "b >= 0");
                                    assert!(b < n_buckets, "b < {}", n_buckets);
                                    b <= min_cost_split_bucket
                                });
                                mid = start + left.len();
                                let combined_len = left.len() + right.len();
                                if combined_len == primitive_info.len() {
                                    primitive_info.clear();
                                    primitive_info.append(&mut left);
                                    primitive_info.append(&mut right);
                                } else {
                                    primitive_info.splice(start..mid, left.iter().cloned());
                                    primitive_info.splice(mid..end, right.iter().cloned());
                                }
                            } else {
                                // create leaf _BVHBuildNode_
                                let first_prim_offset: usize = ordered_prims.len();
                                for item in primitive_info.iter().take(end).skip(start) {
                                    let prim_num: usize = item.primitive_number;
                                    ordered_prims.push(bvh.primitives[prim_num].clone());
                                }
                                node.init_leaf(first_prim_offset, n_primitives, &bounds);
                                return node;
                            }
                        }
                    }
                }
                // make sure we get result for c1 before c0
                let c1 = BVHAccel::recursive_build(
                    bvh.clone(),
                    arena,
                    primitive_info,
                    mid,
                    end,
                    total_nodes,
                    ordered_prims,
                );
                let c0 = BVHAccel::recursive_build(
                    bvh,
                    arena,
                    primitive_info,
                    start,
                    mid,
                    total_nodes,
                    ordered_prims,
                );
                node.init_interior(dim, c0, c1);
            }
        }
        node
    }
    pub fn flatten_bvh_tree<'a>(
        node: &BVHBuildNode<'a>,
        nodes: &mut Vec<LinearBVHNode>,
        offset: &mut usize,
    ) -> usize {
        let my_offset: usize = *offset;
        *offset += 1;
        if node.n_primitives > 0 {
            // leaf
            let linear_node = LinearBVHNode {
                bounds: node.bounds,
                offset: node.first_prim_offset as i32,
                n_primitives: node.n_primitives as u16,
                axis: 0_u8,
                pad: 0_u8,
            };
            nodes[my_offset] = linear_node;
        } else {
            // interior
            if let Some(ref child1) = node.child1 {
                BVHAccel::flatten_bvh_tree(child1, nodes, offset);
            }
            if let Some(ref child2) = node.child2 {
                let linear_node = LinearBVHNode {
                    bounds: node.bounds,
                    offset: BVHAccel::flatten_bvh_tree(child2, nodes, offset) as i32,
                    n_primitives: 0_u16,
                    axis: node.split_axis,
                    pad: 0_u8,
                };
                nodes[my_offset] = linear_node;
            }
        }
        my_offset
    }
    // Primitive
    pub fn world_bound(&self) -> Bounds3f {
        if !self.nodes.is_empty() {
            self.nodes[0].bounds
        } else {
            Bounds3f::default()
        }
    }
    pub fn intersect(&self, ray: &Ray, isect: &mut SurfaceInteraction) -> bool {
        if self.nodes.is_empty() {
            return false;
        }
        // TODO: ProfilePhase p(Prof::AccelIntersect);
        let mut hit: bool = false;
        let inv_dir: Vector3f = Vector3f {
            x: 1.0 / ray.d.x,
            y: 1.0 / ray.d.y,
            z: 1.0 / ray.d.z,
        };
        let dir_is_neg: [u8; 3] = [
            (inv_dir.x < 0.0) as u8,
            (inv_dir.y < 0.0) as u8,
            (inv_dir.z < 0.0) as u8,
        ];
        // follow ray through BVH nodes to find primitive intersections
        let mut to_visit_offset: u32 = 0;
        let mut current_node_index: u32 = 0;
        let mut nodes_to_visit: [u32; 64] = [0_u32; 64];
        loop {
            let node: &LinearBVHNode = &self.nodes[current_node_index as usize];
            // check ray against BVH node
            if node.bounds.intersect_p(ray, &inv_dir, &dir_is_neg) {
                if node.n_primitives > 0 {
                    // intersect ray with primitives in leaf BVH node
                    for i in 0..node.n_primitives {
                        // see primitive.h GeometricPrimitive::Intersect() ...
                        if self.primitives[node.offset as usize + i as usize].intersect(ray, isect)
                        {
                            // TODO: CHECK_GE(...)
                            hit = true;
                        }
                    }
                    if to_visit_offset == 0_u32 {
                        break;
                    }
                    to_visit_offset -= 1_u32;
                    current_node_index = nodes_to_visit[to_visit_offset as usize];
                } else {
                    // put far BVH node on _nodesToVisit_ stack,
                    // advance to near node
                    if dir_is_neg[node.axis as usize] == 1_u8 {
                        nodes_to_visit[to_visit_offset as usize] = current_node_index + 1_u32;
                        to_visit_offset += 1_u32;
                        current_node_index = node.offset as u32;
                    } else {
                        nodes_to_visit[to_visit_offset as usize] = node.offset as u32;
                        to_visit_offset += 1_u32;
                        current_node_index += 1_u32;
                    }
                }
            } else {
                if to_visit_offset == 0_u32 {
                    break;
                }
                to_visit_offset -= 1_u32;
                current_node_index = nodes_to_visit[to_visit_offset as usize];
            }
        }
        hit
    }
    pub fn intersect_p(&self, ray: &Ray) -> bool {
        if self.nodes.is_empty() {
            return false;
        }
        // TODO: ProfilePhase p(Prof::AccelIntersectP);
        let inv_dir: Vector3f = Vector3f {
            x: 1.0 / ray.d.x,
            y: 1.0 / ray.d.y,
            z: 1.0 / ray.d.z,
        };
        let dir_is_neg: [u8; 3] = [
            (inv_dir.x < 0.0) as u8,
            (inv_dir.y < 0.0) as u8,
            (inv_dir.z < 0.0) as u8,
        ];
        let mut to_visit_offset: u32 = 0;
        let mut current_node_index: u32 = 0;
        let mut nodes_to_visit: [u32; 64] = [0_u32; 64];
        loop {
            let node: &LinearBVHNode = &self.nodes[current_node_index as usize];
            if node.bounds.intersect_p(ray, &inv_dir, &dir_is_neg) {
                // process BVH node _node_ for traversal
                if node.n_primitives > 0 {
                    for i in 0..node.n_primitives {
                        if self.primitives[node.offset as usize + i as usize].intersect_p(ray) {
                            return true;
                        }
                    }
                    if to_visit_offset == 0_u32 {
                        break;
                    }
                    to_visit_offset -= 1_u32;
                    current_node_index = nodes_to_visit[to_visit_offset as usize];
                } else if dir_is_neg[node.axis as usize] == 1_u8 {
                    nodes_to_visit[to_visit_offset as usize] = current_node_index + 1_u32;
                    to_visit_offset += 1_u32;
                    current_node_index = node.offset as u32;
                } else {
                    nodes_to_visit[to_visit_offset as usize] = node.offset as u32;
                    to_visit_offset += 1_u32;
                    current_node_index += 1_u32;
                }
            } else {
                if to_visit_offset == 0_u32 {
                    break;
                }
                to_visit_offset -= 1_u32;
                current_node_index = nodes_to_visit[to_visit_offset as usize];
            }
        }
        false
    }
    pub fn get_material(&self) -> Option<Arc<Material>> {
        None
    }
    pub fn get_area_light(&self) -> Option<Arc<Light>> {
        None
    }
}
