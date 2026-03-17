use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{BufReader, Cursor};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use capy_core::{MATERIAL_COLORS, MATERIAL_PALETTE_SIZE, MaterialId, closest_material};
use fbxcel_dom::any::AnyDocument;
use fbxcel_dom::v7400::data::mesh::layer::{
    TypedLayerElementHandle, color::Colors, material::Materials, uv::Uv,
};
use fbxcel_dom::v7400::data::mesh::{TriangleVertexIndex, TriangleVertices};
use fbxcel_dom::v7400::data::texture::WrapMode;
use fbxcel_dom::v7400::object::TypedObjectHandle;
use fbxcel_dom::v7400::object::material::MaterialHandle;
use fbxcel_dom::v7400::object::property::loaders::F64Arr3Loader;
use fbxcel_dom::v7400::object::texture::TextureHandle;
use glam::{DMat4, DQuat, DVec3, EulerRot, Vec3};
use image::RgbaImage;

use crate::error::{AssetError, Result};
use crate::world_format::FileSystem;

const MIN_IMPORT_RESOLUTION: u32 = 1;
const TRIANGLE_EPSILON: f32 = 1.0e-6;
const TRIANGLE_TEXTURE_SAMPLES: [[f32; 3]; 4] = [
    [1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0],
    [0.6, 0.2, 0.2],
    [0.2, 0.6, 0.2],
    [0.2, 0.2, 0.6],
];
const ALPHA_CUTOFF: f32 = 0.1;

#[derive(Debug, Clone)]
pub struct VoxelPrefabAsset {
    pub name: String,
    pub source_path: PathBuf,
    pub import_resolution: u32,
    pub size: [u32; 3],
    pub anchor: [i32; 3],
    pub filled_voxel_count: usize,
    pub voxels: Vec<MaterialId>,
}

impl VoxelPrefabAsset {
    pub fn voxel(&self, x: u32, y: u32, z: u32) -> MaterialId {
        let index = voxel_index(self.size, x, y, z);
        self.voxels[index]
    }
}

#[derive(Clone, Copy)]
struct MeshTriangle {
    positions: [Vec3; 3],
    color: [f32; 3],
}

struct TriangleMesh {
    name: String,
    triangles: Vec<MeshTriangle>,
    bounds_min: Vec3,
    bounds_max: Vec3,
}

#[derive(Clone)]
struct TextureSampler {
    image: Rc<RgbaImage>,
    uv_set: Option<String>,
    uv_swap: bool,
    translation: [f32; 2],
    scaling: [f32; 2],
    wrap_u: WrapMode,
    wrap_v: WrapMode,
}

#[derive(Clone)]
struct ImportedMaterial {
    diffuse_color: Option<[f32; 3]>,
    texture: Option<TextureSampler>,
}

struct UvLayer<'a> {
    name: Option<String>,
    uv: Uv<'a>,
}

pub fn import_fbx_prefab(
    path: &Path,
    resolution: u32,
    fs: &impl FileSystem,
) -> Result<VoxelPrefabAsset> {
    if resolution < MIN_IMPORT_RESOLUTION {
        return Err(AssetError::InvalidPrefabResolution(resolution));
    }

    let bytes = fs.read(path)?;
    let reader = BufReader::new(Cursor::new(bytes));
    let doc = match AnyDocument::from_seekable_reader(reader).map_err(|err| {
        AssetError::FbxImportFailed {
            path: path.to_path_buf(),
            reason: err.to_string(),
        }
    })? {
        AnyDocument::V7400(_, doc) => doc,
        _ => {
            return Err(AssetError::FbxImportFailed {
                path: path.to_path_buf(),
                reason: String::from("unsupported FBX document version"),
            });
        }
    };

    let mesh = extract_triangle_mesh(path, &doc, fs)?;
    voxelize_mesh(mesh, path, resolution)
}

fn extract_triangle_mesh(
    path: &Path,
    doc: &fbxcel_dom::v7400::Document,
    fs: &impl FileSystem,
) -> Result<TriangleMesh> {
    let mut mesh_name = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(String::from)
        .unwrap_or_else(|| String::from("prefab"));
    let mut triangles = Vec::new();
    let mut bounds_min = Vec3::splat(f32::INFINITY);
    let mut bounds_max = Vec3::splat(f32::NEG_INFINITY);
    let mut texture_cache = HashMap::new();

    for object in doc.objects() {
        let TypedObjectHandle::Model(fbxcel_dom::v7400::object::model::TypedModelHandle::Mesh(
            model_mesh,
        )) = object.get_typed()
        else {
            continue;
        };

        if let Some(name) = model_mesh.name()
            && !name.is_empty()
            && mesh_name == "prefab"
        {
            mesh_name = name.to_string();
        }

        let geometry = model_mesh
            .geometry()
            .map_err(|err| AssetError::FbxImportFailed {
                path: path.to_path_buf(),
                reason: err.to_string(),
            })?;
        let polygon_vertices =
            geometry
                .polygon_vertices()
                .map_err(|err| AssetError::FbxImportFailed {
                    path: path.to_path_buf(),
                    reason: err.to_string(),
                })?;
        let triangle_vertices = polygon_vertices
            .triangulate_each(|_, polygon, out| {
                if polygon.len() < 3 {
                    return Ok(());
                }
                for index in 1..(polygon.len() - 1) {
                    out.push([polygon[0], polygon[index], polygon[index + 1]]);
                }
                Ok(())
            })
            .map_err(|err| AssetError::FbxImportFailed {
                path: path.to_path_buf(),
                reason: err.to_string(),
            })?;
        let triangle_indices: Vec<_> = triangle_vertices.triangle_vertex_indices().collect();
        if triangle_indices.is_empty() {
            continue;
        }

        let (color_layer, material_layer) = mesh_layers(&geometry);
        let uv_layers = collect_uv_layers(&geometry);
        eprintln!(
            "[prefab_import] uv_layers: {} (names: {:?})",
            uv_layers.len(),
            uv_layers
                .iter()
                .map(|l| l.name.as_deref().unwrap_or("?"))
                .collect::<Vec<_>>(),
        );
        let materials = collect_model_materials(model_mesh, path, fs, &mut texture_cache);
        let fallback_material = materials
            .first()
            .and_then(|material| material.diffuse_color)
            .map(color_to_material)
            .unwrap_or_else(|| color_to_material([0.8, 0.8, 0.8]));
        let world_transform = model_world_transform(model_mesh);
        let mut _debug_tri_count = 0u32;

        for tri in triangle_indices.chunks_exact(3) {
            let Some(p0) = triangle_vertices.control_point(tri[0]) else {
                continue;
            };
            let Some(p1) = triangle_vertices.control_point(tri[1]) else {
                continue;
            };
            let Some(p2) = triangle_vertices.control_point(tri[2]) else {
                continue;
            };

            let transformed = [
                world_transform
                    .transform_point3(DVec3::new(p0.x, p0.y, p0.z))
                    .as_vec3(),
                world_transform
                    .transform_point3(DVec3::new(p1.x, p1.y, p1.z))
                    .as_vec3(),
                world_transform
                    .transform_point3(DVec3::new(p2.x, p2.y, p2.z))
                    .as_vec3(),
            ];

            let area = (transformed[1] - transformed[0])
                .cross(transformed[2] - transformed[0])
                .length_squared();
            if area <= TRIANGLE_EPSILON {
                continue;
            }

            let color_opt = triangle_color(
                &uv_layers,
                color_layer.as_ref(),
                material_layer.as_ref(),
                &materials,
                &triangle_vertices,
                tri,
            );
            if _debug_tri_count < 5 {
                eprintln!(
                    "[prefab_import] tri[{_debug_tri_count}] color={color_opt:?}, has_material_layer={}, has_color_layer={}",
                    material_layer.is_some(),
                    color_layer.is_some(),
                );
            }
            _debug_tri_count += 1;
            let color = color_opt.unwrap_or_else(|| MATERIAL_COLORS[fallback_material as usize]);

            for position in transformed {
                bounds_min = bounds_min.min(position);
                bounds_max = bounds_max.max(position);
            }

            triangles.push(MeshTriangle {
                positions: transformed,
                color,
            });
        }
    }

    if triangles.is_empty() {
        return Err(AssetError::NoMeshGeometry(path.to_path_buf()));
    }

    Ok(TriangleMesh {
        name: mesh_name,
        triangles,
        bounds_min,
        bounds_max,
    })
}

fn mesh_layers<'a>(
    geometry: &'a fbxcel_dom::v7400::object::geometry::MeshHandle<'a>,
) -> (Option<Colors<'a>>, Option<Materials<'a>>) {
    let mut color_layer = None;
    let mut material_layer = None;

    for layer in geometry.layers() {
        for entry in layer.layer_element_entries() {
            let Ok(handle) = entry.typed_layer_element() else {
                continue;
            };

            match handle {
                TypedLayerElementHandle::Color(color) if color_layer.is_none() => {
                    color_layer = color.color().ok();
                }
                TypedLayerElementHandle::Material(material) if material_layer.is_none() => {
                    material_layer = material.materials().ok();
                }
                _ => {}
            }

            if color_layer.is_some() && material_layer.is_some() {
                return (color_layer, material_layer);
            }
        }
    }

    (color_layer, material_layer)
}

fn collect_uv_layers<'a>(
    geometry: &'a fbxcel_dom::v7400::object::geometry::MeshHandle<'a>,
) -> Vec<UvLayer<'a>> {
    let mut uv_layers = Vec::new();

    for layer in geometry.layers() {
        for entry in layer.layer_element_entries() {
            let Ok(handle) = entry.typed_layer_element() else {
                continue;
            };

            let TypedLayerElementHandle::Uv(uv_handle) = handle else {
                continue;
            };

            let Some(uv) = uv_handle.uv().ok() else {
                continue;
            };
            uv_layers.push(UvLayer {
                name: uv_handle.name().ok().map(str::to_string),
                uv,
            });
        }
    }

    uv_layers
}

fn collect_model_materials(
    model_mesh: fbxcel_dom::v7400::object::model::MeshHandle<'_>,
    fbx_path: &Path,
    fs: &impl FileSystem,
    texture_cache: &mut HashMap<PathBuf, Rc<RgbaImage>>,
) -> Vec<ImportedMaterial> {
    let mut materials = Vec::new();
    // Cache already-imported materials by object ID so duplicate slots reuse
    // the same ImportedMaterial without reloading textures, but still push a
    // separate entry for each slot to preserve index alignment.
    let mut imported: HashMap<i64, ImportedMaterial> = HashMap::new();

    for connected in model_mesh.source_objects() {
        let Some(object) = connected.object_handle() else {
            continue;
        };
        let TypedObjectHandle::Material(material) = object.get_typed() else {
            continue;
        };

        let id = material.object_id().raw();
        let entry = imported.entry(id).or_insert_with(|| ImportedMaterial {
            diffuse_color: material_diffuse_color(&material),
            texture: load_material_texture(&material, fbx_path, fs, texture_cache),
        });
        materials.push(entry.clone());
    }

    materials
}

fn model_world_transform(model_mesh: fbxcel_dom::v7400::object::model::MeshHandle<'_>) -> DMat4 {
    let mut chain = vec![*model_mesh];
    let mut current = model_mesh.parent_model();
    while let Some(parent) = current {
        chain.push(*parent);
        current = parent.parent_model();
    }

    chain.into_iter().rev().fold(DMat4::IDENTITY, |acc, model| {
        acc * model_local_transform(model)
    })
}

fn model_local_transform(model: fbxcel_dom::v7400::object::model::ModelHandle<'_>) -> DMat4 {
    let props = model.properties_by_native_typename("FbxNode");

    let translation = read_property_vec3(&props, "Lcl Translation", [0.0, 0.0, 0.0]);
    let rotation = read_property_vec3(&props, "Lcl Rotation", [0.0, 0.0, 0.0]);
    let scale = read_property_vec3(&props, "Lcl Scaling", [1.0, 1.0, 1.0]);
    let geometric_translation = read_property_vec3(&props, "GeometricTranslation", [0.0, 0.0, 0.0]);
    let geometric_rotation = read_property_vec3(&props, "GeometricRotation", [0.0, 0.0, 0.0]);
    let geometric_scale = read_property_vec3(&props, "GeometricScaling", [1.0, 1.0, 1.0]);

    compose_transform(translation, rotation, scale)
        * compose_transform(geometric_translation, geometric_rotation, geometric_scale)
}

fn read_property_vec3(
    props: &fbxcel_dom::v7400::object::property::ObjectProperties<'_>,
    name: &str,
    default: [f64; 3],
) -> [f64; 3] {
    props
        .get_property(name)
        .and_then(|property| property.load_value(F64Arr3Loader::new()).ok())
        .unwrap_or(default)
}

fn compose_transform(translation: [f64; 3], rotation_deg: [f64; 3], scale: [f64; 3]) -> DMat4 {
    let translation = DVec3::from_array(translation);
    let rotation = DQuat::from_euler(
        EulerRot::XYZ,
        rotation_deg[0].to_radians(),
        rotation_deg[1].to_radians(),
        rotation_deg[2].to_radians(),
    );
    let scale = DVec3::from_array(scale);

    DMat4::from_translation(translation) * DMat4::from_quat(rotation) * DMat4::from_scale(scale)
}

fn average_vertex_color(
    colors: &Colors<'_>,
    triangles: &TriangleVertices<'_>,
    tri: &[TriangleVertexIndex],
) -> Option<[f32; 3]> {
    let mut sum = [0.0f32; 3];
    let mut count = 0.0f32;

    for index in tri {
        let rgba = colors.color(triangles, *index).ok()?;
        sum[0] += rgba[0] as f32;
        sum[1] += rgba[1] as f32;
        sum[2] += rgba[2] as f32;
        count += 1.0;
    }

    if count == 0.0 {
        None
    } else {
        Some(normalize_color([
            sum[0] / count,
            sum[1] / count,
            sum[2] / count,
        ]))
    }
}

fn triangle_color(
    uv_layers: &[UvLayer<'_>],
    colors: Option<&Colors<'_>>,
    material_layer: Option<&Materials<'_>>,
    materials: &[ImportedMaterial],
    triangles: &TriangleVertices<'_>,
    tri: &[TriangleVertexIndex],
) -> Option<[f32; 3]> {
    let vertex_color = colors.and_then(|colors| average_vertex_color(colors, triangles, tri));
    let material = material_for_triangle(material_layer, materials, triangles, tri)?;
    let texture_color = material
        .texture
        .as_ref()
        .and_then(|texture| sample_triangle_texture(texture, uv_layers, triangles, tri));

    match (texture_color, vertex_color) {
        (Some(texture), Some(vertex)) => Some(multiply_colors(texture, vertex)),
        (Some(texture), None) => Some(texture),
        (None, Some(vertex)) => Some(vertex),
        (None, None) => material.diffuse_color,
    }
}

fn material_for_triangle<'a>(
    material_layer: Option<&Materials<'_>>,
    materials: &'a [ImportedMaterial],
    triangles: &TriangleVertices<'_>,
    tri: &[TriangleVertexIndex],
) -> Option<&'a ImportedMaterial> {
    if materials.is_empty() {
        return None;
    }

    if let Some(material_layer) = material_layer {
        let tri_vertex = *tri.first()?;
        let material_index = material_layer.material_index(triangles, tri_vertex).ok()?;
        let idx = material_index.to_u32() as usize;
        // Fall back to first material when index is out of range (buggy FBX exports).
        Some(materials.get(idx).unwrap_or(&materials[0]))
    } else {
        materials.first()
    }
}

fn material_diffuse_color(material: &MaterialHandle<'_>) -> Option<[f32; 3]> {
    material
        .properties()
        .diffuse_color_or_default()
        .ok()
        .map(|diffuse| normalize_color([diffuse.r as f32, diffuse.g as f32, diffuse.b as f32]))
}

fn load_material_texture(
    material: &MaterialHandle<'_>,
    fbx_path: &Path,
    fs: &impl FileSystem,
    texture_cache: &mut HashMap<PathBuf, Rc<RgbaImage>>,
) -> Option<TextureSampler> {
    let texture = select_base_color_texture(material)?;
    load_texture_sampler(texture, fbx_path, fs, texture_cache)
}

fn select_base_color_texture<'a>(material: &'a MaterialHandle<'a>) -> Option<TextureHandle<'a>> {
    let mut best = None;
    let mat_name = material.name().unwrap_or("<unnamed>");

    for connected in material.source_objects() {
        let Some(object) = connected.object_handle() else {
            continue;
        };
        let TypedObjectHandle::Texture(texture) = object.get_typed() else {
            continue;
        };

        let label = connected.label().unwrap_or_default();
        let name = texture.name().unwrap_or_default();
        let clip_name = texture
            .video_clip()
            .and_then(|clip| clip.relative_filename().ok())
            .unwrap_or_default();
        let score = texture_score(label) + texture_score(name) + texture_score(clip_name);

        eprintln!(
            "[prefab_import] texture candidate for {mat_name:?}: label={label:?} name={name:?} clip={clip_name:?} score={score}",
        );

        if best
            .as_ref()
            .is_none_or(|(best_score, _)| score > *best_score)
        {
            best = Some((score, texture));
        }
    }

    let result = best.and_then(
        |(score, texture)| {
            if score < -100 { None } else { Some(texture) }
        },
    );
    eprintln!(
        "[prefab_import] select_base_color_texture for {mat_name:?}: found={}",
        result.is_some(),
    );
    result
}

fn texture_score(value: &str) -> i32 {
    let lower = value.to_ascii_lowercase();
    let mut score = 0;

    if lower.contains("base_color") || lower.contains("basecolor") || lower.contains("albedo") {
        score += 100;
    }
    if lower.contains("diffuse") || lower.contains("color") {
        score += 50;
    }
    if lower.contains("alpha") || lower.contains("opacity") || lower.contains("mask") {
        score -= 80;
    }
    if lower.contains("normal")
        || lower.contains("rough")
        || lower.contains("metal")
        || lower.contains("spec")
        || lower.contains("bump")
        || lower.contains("ao")
    {
        score -= 120;
    }

    score
}

fn load_texture_sampler(
    texture: TextureHandle<'_>,
    fbx_path: &Path,
    fs: &impl FileSystem,
    texture_cache: &mut HashMap<PathBuf, Rc<RgbaImage>>,
) -> Option<TextureSampler> {
    let image = load_texture_image(texture, fbx_path, fs, texture_cache)?;
    let props = texture.properties();
    let translation = props.translation_or_default().ok()?;
    let scaling = props.scaling_or_default().ok()?;

    Some(TextureSampler {
        image,
        uv_set: props.uv_set().ok().flatten().map(str::to_string),
        uv_swap: props.uv_swap_or_default().ok().unwrap_or(false),
        translation: [translation.x as f32, translation.y as f32],
        scaling: [scaling.x as f32, scaling.y as f32],
        wrap_u: props
            .wrap_mode_u_or_default()
            .ok()
            .unwrap_or(WrapMode::Repeat),
        wrap_v: props
            .wrap_mode_v_or_default()
            .ok()
            .unwrap_or(WrapMode::Repeat),
    })
}

fn load_texture_image(
    texture: TextureHandle<'_>,
    fbx_path: &Path,
    fs: &impl FileSystem,
    texture_cache: &mut HashMap<PathBuf, Rc<RgbaImage>>,
) -> Option<Rc<RgbaImage>> {
    let clip = texture.video_clip()?;

    // Try embedded content first, but fall through to file path if decode fails.
    if let Some(content) = clip.content() {
        eprintln!("[prefab_import] embedded content: {} bytes", content.len(),);
        match image::load_from_memory(content) {
            Ok(image) => return Some(Rc::new(image.to_rgba8())),
            Err(err) => eprintln!("[prefab_import] embedded decode failed: {err}"),
        }
    }

    let raw_path_result = clip.relative_filename();
    eprintln!("[prefab_import] relative_filename: {raw_path_result:?}");
    let raw_path = raw_path_result.ok()?;
    let resolved_path = resolve_texture_path(fbx_path, raw_path, fs);
    eprintln!("[prefab_import] resolved_path: {resolved_path:?}");
    let resolved_path = resolved_path?;

    if let Some(image) = texture_cache.get(&resolved_path) {
        return Some(Rc::clone(image));
    }

    let bytes = fs.read(&resolved_path).ok()?;
    let image = Rc::new(image::load_from_memory(&bytes).ok()?.to_rgba8());
    texture_cache.insert(resolved_path, Rc::clone(&image));
    Some(image)
}

fn resolve_texture_path(fbx_path: &Path, raw_path: &str, fs: &impl FileSystem) -> Option<PathBuf> {
    let original = Path::new(raw_path);
    let basename = original.file_name()?;
    let mut seen = HashSet::new();
    let mut candidates = Vec::new();

    if original.is_absolute() {
        candidates.push(original.to_path_buf());
    }

    if let Some(parent) = fbx_path.parent() {
        candidates.push(parent.join(original));

        for ancestor in parent.ancestors() {
            candidates.push(ancestor.join(basename));
            candidates.push(ancestor.join("Textures").join(basename));
            candidates.push(ancestor.join("textures").join(basename));
        }
    }

    for candidate in candidates {
        if !seen.insert(candidate.clone()) {
            continue;
        }
        if fs.exists(&candidate) {
            return Some(candidate);
        }
    }

    None
}

fn sample_triangle_texture(
    texture: &TextureSampler,
    uv_layers: &[UvLayer<'_>],
    triangles: &TriangleVertices<'_>,
    tri: &[TriangleVertexIndex],
) -> Option<[f32; 3]> {
    let uv_layer = select_uv_layer(texture, uv_layers)?;
    let uv0 = uv_layer.uv.uv(triangles, tri[0]).ok()?;
    let uv1 = uv_layer.uv.uv(triangles, tri[1]).ok()?;
    let uv2 = uv_layer.uv.uv(triangles, tri[2]).ok()?;
    let uv_triangle = [
        [uv0.x as f32, uv0.y as f32],
        [uv1.x as f32, uv1.y as f32],
        [uv2.x as f32, uv2.y as f32],
    ];

    let mut sum = [0.0f32; 3];
    let mut count = 0.0f32;

    for bary in TRIANGLE_TEXTURE_SAMPLES {
        let uv = [
            uv_triangle[0][0] * bary[0] + uv_triangle[1][0] * bary[1] + uv_triangle[2][0] * bary[2],
            uv_triangle[0][1] * bary[0] + uv_triangle[1][1] * bary[1] + uv_triangle[2][1] * bary[2],
        ];
        let sampled = sample_texture(texture, uv);
        if sampled[3] <= ALPHA_CUTOFF {
            continue;
        }

        sum[0] += sampled[0];
        sum[1] += sampled[1];
        sum[2] += sampled[2];
        count += 1.0;
    }

    if count == 0.0 {
        None
    } else {
        Some(normalize_color([
            sum[0] / count,
            sum[1] / count,
            sum[2] / count,
        ]))
    }
}

fn select_uv_layer<'a>(
    texture: &TextureSampler,
    uv_layers: &'a [UvLayer<'a>],
) -> Option<&'a UvLayer<'a>> {
    if let Some(uv_set) = texture.uv_set.as_deref() {
        uv_layers
            .iter()
            .find(|layer| layer.name.as_deref() == Some(uv_set))
            .or_else(|| {
                uv_layers
                    .iter()
                    .find(|layer| layer.name.as_deref() == Some("default"))
            })
            .or_else(|| uv_layers.first())
    } else {
        uv_layers.first()
    }
}

fn sample_texture(texture: &TextureSampler, uv: [f32; 2]) -> [f32; 4] {
    let mut u = uv[0] * texture.scaling[0] + texture.translation[0];
    let mut v = uv[1] * texture.scaling[1] + texture.translation[1];

    if texture.uv_swap {
        std::mem::swap(&mut u, &mut v);
    }

    u = wrap_uv(u, texture.wrap_u);
    v = wrap_uv(v, texture.wrap_v);
    v = 1.0 - v;

    let width = texture.image.width().max(1);
    let height = texture.image.height().max(1);
    let x = (u * (width - 1) as f32)
        .round()
        .clamp(0.0, (width - 1) as f32) as u32;
    let y = (v * (height - 1) as f32)
        .round()
        .clamp(0.0, (height - 1) as f32) as u32;
    let pixel = texture.image.get_pixel(x, y).0;

    [
        pixel[0] as f32 / 255.0,
        pixel[1] as f32 / 255.0,
        pixel[2] as f32 / 255.0,
        pixel[3] as f32 / 255.0,
    ]
}

fn wrap_uv(value: f32, mode: WrapMode) -> f32 {
    match mode {
        WrapMode::Repeat => value.rem_euclid(1.0),
        WrapMode::Clamp => value.clamp(0.0, 1.0),
    }
}

fn multiply_colors(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    normalize_color([a[0] * b[0], a[1] * b[1], a[2] * b[2]])
}

fn normalize_color(color: [f32; 3]) -> [f32; 3] {
    let max_channel = color[0].max(color[1]).max(color[2]);
    if max_channel > 1.0 && max_channel <= 255.0 {
        [
            (color[0] / 255.0).clamp(0.0, 1.0),
            (color[1] / 255.0).clamp(0.0, 1.0),
            (color[2] / 255.0).clamp(0.0, 1.0),
        ]
    } else {
        [
            color[0].clamp(0.0, 1.0),
            color[1].clamp(0.0, 1.0),
            color[2].clamp(0.0, 1.0),
        ]
    }
}

fn color_to_material(color: [f32; 3]) -> MaterialId {
    let color = normalize_color(color);
    closest_material(color)
}

fn voxelize_mesh(mesh: TriangleMesh, path: &Path, resolution: u32) -> Result<VoxelPrefabAsset> {
    let extent = (mesh.bounds_max - mesh.bounds_min).max(Vec3::splat(0.0));
    let longest_axis = extent.max_element();
    if longest_axis <= TRIANGLE_EPSILON {
        return Err(AssetError::EmptyPrefab(path.to_path_buf()));
    }

    let voxel_size = longest_axis / resolution as f32;
    if voxel_size <= TRIANGLE_EPSILON {
        return Err(AssetError::EmptyPrefab(path.to_path_buf()));
    }

    let size = [
        grid_extent(extent.x, voxel_size),
        grid_extent(extent.y, voxel_size),
        grid_extent(extent.z, voxel_size),
    ];
    let total_voxels = size[0] as u64 * size[1] as u64 * size[2] as u64;
    if total_voxels == 0 || total_voxels > usize::MAX as u64 {
        return Err(AssetError::PrefabTooLarge(total_voxels));
    }

    let mut voxels = vec![0 as MaterialId; total_voxels as usize];
    // Sparse map: voxel index → (hit_count, accumulated RGB color).
    // Only surface voxels get entries, saving ~28 MB of dense arrays at 128^3.
    let mut surface_hits: HashMap<usize, (u16, [f32; 3])> = HashMap::new();
    let half = Vec3::splat(voxel_size * 0.5);

    for triangle in &mesh.triangles {
        let local = [
            triangle.positions[0] - mesh.bounds_min,
            triangle.positions[1] - mesh.bounds_min,
            triangle.positions[2] - mesh.bounds_min,
        ];
        let tri_min = local[0].min(local[1]).min(local[2]);
        let tri_max = local[0].max(local[1]).max(local[2]);

        let min_x = voxel_coord_floor(tri_min.x, voxel_size).clamp(0, size[0] as i32 - 1);
        let min_y = voxel_coord_floor(tri_min.y, voxel_size).clamp(0, size[1] as i32 - 1);
        let min_z = voxel_coord_floor(tri_min.z, voxel_size).clamp(0, size[2] as i32 - 1);
        let max_x = voxel_coord_floor(tri_max.x, voxel_size).clamp(0, size[0] as i32 - 1);
        let max_y = voxel_coord_floor(tri_max.y, voxel_size).clamp(0, size[1] as i32 - 1);
        let max_z = voxel_coord_floor(tri_max.z, voxel_size).clamp(0, size[2] as i32 - 1);

        for z in min_z..=max_z {
            for y in min_y..=max_y {
                for x in min_x..=max_x {
                    let center = Vec3::new(
                        (x as f32 + 0.5) * voxel_size,
                        (y as f32 + 0.5) * voxel_size,
                        (z as f32 + 0.5) * voxel_size,
                    );
                    if !triangle_box_overlap(local, center, half) {
                        continue;
                    }

                    let index = voxel_index(size, x as u32, y as u32, z as u32);
                    let entry = surface_hits.entry(index).or_insert((0, [0.0; 3]));
                    entry.0 = entry.0.saturating_add(1);
                    entry.1[0] += triangle.color[0];
                    entry.1[1] += triangle.color[1];
                    entry.1[2] += triangle.color[2];
                }
            }
        }
    }

    let mut material_counts = [0u32; MATERIAL_PALETTE_SIZE];
    for (&index, &(count, color_sum)) in &surface_hits {
        let inv = 1.0 / count as f32;
        let color = [color_sum[0] * inv, color_sum[1] * inv, color_sum[2] * inv];
        let material = color_to_material(color);
        voxels[index] = material;
        material_counts[material as usize] += 1;
    }

    let fill_material = dominant_material(&material_counts).unwrap_or(1);
    fill_enclosed_voxels(size, fill_material, &mut voxels);

    let filled_voxel_count = voxels.iter().filter(|&&material| material != 0).count();
    if filled_voxel_count == 0 {
        return Err(AssetError::EmptyPrefab(path.to_path_buf()));
    }

    Ok(VoxelPrefabAsset {
        name: mesh.name,
        source_path: path.to_path_buf(),
        import_resolution: resolution,
        size,
        anchor: [size[0] as i32 / 2, 0, size[2] as i32 / 2],
        filled_voxel_count,
        voxels,
    })
}

fn dominant_material(counts: &[u32; MATERIAL_PALETTE_SIZE]) -> Option<MaterialId> {
    counts
        .iter()
        .enumerate()
        .skip(1)
        .max_by_key(|(_, count)| *count)
        .and_then(|(index, count)| {
            if *count > 0 {
                Some(index as MaterialId)
            } else {
                None
            }
        })
}

fn grid_extent(extent: f32, voxel_size: f32) -> u32 {
    ((extent / voxel_size).ceil() as u32).max(1)
}

fn voxel_coord_floor(value: f32, voxel_size: f32) -> i32 {
    (value / voxel_size).floor() as i32
}

fn fill_enclosed_voxels(size: [u32; 3], material: MaterialId, voxels: &mut [MaterialId]) {
    let padded = [size[0] + 2, size[1] + 2, size[2] + 2];
    let padded_len = padded[0] as usize * padded[1] as usize * padded[2] as usize;
    // Bitset: 1 bit per cell instead of 1 byte. For 128^3 padded this is
    // ~275 KB instead of ~2.2 MB, with better cache locality for BFS.
    let mut visited = vec![0u64; padded_len.div_ceil(64)];
    let mut queue = VecDeque::new();

    let start_idx = padded_index(padded, 0, 0, 0);
    visited[start_idx / 64] |= 1u64 << (start_idx % 64);
    queue.push_back([0u32, 0u32, 0u32]);

    while let Some([x, y, z]) = queue.pop_front() {
        for [nx, ny, nz] in neighbors([x, y, z], padded) {
            let visit_index = padded_index(padded, nx, ny, nz);
            let word = visit_index / 64;
            let bit = 1u64 << (visit_index % 64);
            if visited[word] & bit != 0 || padded_is_solid(size, voxels, nx, ny, nz) {
                continue;
            }

            visited[word] |= bit;
            queue.push_back([nx, ny, nz]);
        }
    }

    for z in 0..size[2] {
        for y in 0..size[1] {
            for x in 0..size[0] {
                let pi = padded_index(padded, x + 1, y + 1, z + 1);
                if visited[pi / 64] & (1u64 << (pi % 64)) != 0 {
                    continue;
                }

                let index = voxel_index(size, x, y, z);
                if voxels[index] == 0 {
                    voxels[index] = material;
                }
            }
        }
    }
}

fn padded_is_solid(size: [u32; 3], voxels: &[MaterialId], x: u32, y: u32, z: u32) -> bool {
    if x == 0 || y == 0 || z == 0 || x > size[0] || y > size[1] || z > size[2] {
        return false;
    }

    voxels[voxel_index(size, x - 1, y - 1, z - 1)] != 0
}

fn neighbors(cell: [u32; 3], bounds: [u32; 3]) -> impl Iterator<Item = [u32; 3]> {
    let [x, y, z] = cell;
    [
        x.checked_sub(1).map(|next| [next, y, z]),
        (x + 1 < bounds[0]).then_some([x + 1, y, z]),
        y.checked_sub(1).map(|next| [x, next, z]),
        (y + 1 < bounds[1]).then_some([x, y + 1, z]),
        z.checked_sub(1).map(|next| [x, y, next]),
        (z + 1 < bounds[2]).then_some([x, y, z + 1]),
    ]
    .into_iter()
    .flatten()
}

fn voxel_index(size: [u32; 3], x: u32, y: u32, z: u32) -> usize {
    (x + y * size[0] + z * size[0] * size[1]) as usize
}

fn padded_index(size: [u32; 3], x: u32, y: u32, z: u32) -> usize {
    (x + y * size[0] + z * size[0] * size[1]) as usize
}

fn triangle_box_overlap(triangle: [Vec3; 3], center: Vec3, half: Vec3) -> bool {
    let translated = [
        triangle[0] - center,
        triangle[1] - center,
        triangle[2] - center,
    ];

    for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
        let (min, max) = projected_bounds(&translated, axis);
        let radius = half.x * axis.x.abs() + half.y * axis.y.abs() + half.z * axis.z.abs();
        if min > radius || max < -radius {
            return false;
        }
    }

    let edges = [
        translated[1] - translated[0],
        translated[2] - translated[1],
        translated[0] - translated[2],
    ];

    for edge in edges {
        for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
            let test_axis = edge.cross(axis);
            if test_axis.length_squared() <= TRIANGLE_EPSILON {
                continue;
            }

            let (min, max) = projected_bounds(&translated, test_axis);
            let radius = half.x * test_axis.x.abs()
                + half.y * test_axis.y.abs()
                + half.z * test_axis.z.abs();
            if min > radius || max < -radius {
                return false;
            }
        }
    }

    let normal = edges[0].cross(edges[1]);
    if normal.length_squared() <= TRIANGLE_EPSILON {
        return false;
    }

    plane_box_overlap(normal, translated[0], half)
}

fn projected_bounds(points: &[Vec3; 3], axis: Vec3) -> (f32, f32) {
    let p0 = points[0].dot(axis);
    let p1 = points[1].dot(axis);
    let p2 = points[2].dot(axis);
    (p0.min(p1).min(p2), p0.max(p1).max(p2))
}

fn plane_box_overlap(normal: Vec3, vertex: Vec3, half: Vec3) -> bool {
    let vmin = Vec3::new(
        if normal.x > 0.0 {
            -half.x - vertex.x
        } else {
            half.x - vertex.x
        },
        if normal.y > 0.0 {
            -half.y - vertex.y
        } else {
            half.y - vertex.y
        },
        if normal.z > 0.0 {
            -half.z - vertex.z
        } else {
            half.z - vertex.z
        },
    );
    let vmax = Vec3::new(
        if normal.x > 0.0 {
            half.x - vertex.x
        } else {
            -half.x - vertex.x
        },
        if normal.y > 0.0 {
            half.y - vertex.y
        } else {
            -half.y - vertex.y
        },
        if normal.z > 0.0 {
            half.z - vertex.z
        } else {
            -half.z - vertex.z
        },
    );

    if normal.dot(vmin) > 0.0 {
        return false;
    }

    normal.dot(vmax) >= 0.0
}

#[cfg(test)]
mod tests {
    use super::{MeshTriangle, TriangleMesh, voxelize_mesh};

    use glam::Vec3;

    #[test]
    fn voxelizes_a_unit_cube() {
        let positions = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(1.0, 0.0, 1.0),
            Vec3::new(1.0, 1.0, 1.0),
            Vec3::new(0.0, 1.0, 1.0),
        ];
        let faces = [
            [0, 1, 2],
            [0, 2, 3],
            [4, 6, 5],
            [4, 7, 6],
            [0, 4, 5],
            [0, 5, 1],
            [1, 5, 6],
            [1, 6, 2],
            [2, 6, 7],
            [2, 7, 3],
            [3, 7, 4],
            [3, 4, 0],
        ];

        let mesh = TriangleMesh {
            name: String::from("cube"),
            triangles: faces
                .into_iter()
                .map(|face| MeshTriangle {
                    positions: [positions[face[0]], positions[face[1]], positions[face[2]]],
                    color: [1.0, 0.0, 0.0],
                })
                .collect(),
            bounds_min: Vec3::ZERO,
            bounds_max: Vec3::ONE,
        };

        let prefab =
            voxelize_mesh(mesh, std::path::Path::new("cube.fbx"), 8).expect("cube should voxelize");

        assert_eq!(prefab.size, [8, 8, 8]);
        assert!(prefab.filled_voxel_count > 0);
        assert_eq!(prefab.anchor, [4, 0, 4]);
    }
}
