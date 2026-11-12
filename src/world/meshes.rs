//! Mesh helpers: boxes with world-scaled UVs (so textures tile at a constant
//! real-world size regardless of box dimensions) plus tangents for normal maps.

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};

/// Axis-aligned box centred on the origin. `tile` is the texture size in
/// metres (e.g. 1.5 → texture repeats every 1.5 m).
pub fn tiled_box(size: Vec3, tile: f32) -> Mesh {
    let h = size * 0.5;
    let t = tile.max(0.01);
    // (normal, u axis, v axis) per face; v axis points "down" in texture space.
    let faces: [(Vec3, Vec3, Vec3); 6] = [
        (Vec3::Z, Vec3::X, Vec3::NEG_Y),
        (Vec3::NEG_Z, Vec3::NEG_X, Vec3::NEG_Y),
        (Vec3::X, Vec3::NEG_Z, Vec3::NEG_Y),
        (Vec3::NEG_X, Vec3::Z, Vec3::NEG_Y),
        (Vec3::Y, Vec3::X, Vec3::Z),
        (Vec3::NEG_Y, Vec3::X, Vec3::NEG_Z),
    ];
    let mut positions = Vec::with_capacity(24);
    let mut normals = Vec::with_capacity(24);
    let mut uvs = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    for (n, u, v) in faces {
        let base = positions.len() as u32;
        for (su, sv) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            let p = n * h + u * h * su + v * h * sv;
            positions.push(p.to_array());
            normals.push(n.to_array());
            // World-space UVs: project position onto the face's axes.
            uvs.push([p.dot(u) / t, p.dot(v) / t]);
        }
        // Counter-clockwise when viewed from outside.
        indices.extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
    }
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(Indices::U32(indices));
    let _ = mesh.generate_tangents();
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;

    #[test]
    fn box_winding_faces_outward() {
        let mesh = tiled_box(Vec3::new(2.0, 1.0, 0.5), 1.0);
        let Some(VertexAttributeValues::Float32x3(pos)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else { panic!() };
        let Some(VertexAttributeValues::Float32x3(nrm)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL) else { panic!() };
        let Some(Indices::U32(idx)) = mesh.indices() else { panic!() };
        for tri in idx.chunks(3) {
            let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| Vec3::from(pos[i as usize]));
            let face_n = (b - a).cross(c - a).normalize();
            assert!(face_n.dot(Vec3::from(nrm[tri[0] as usize])) > 0.99);
        }
    }

    #[test]
    fn uvs_scale_with_tile_size() {
        let mesh = tiled_box(Vec3::new(3.0, 3.0, 3.0), 1.5);
        let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0) else { panic!() };
        let span = uv.iter().map(|u| u[0]).fold(f32::MIN, f32::max) - uv.iter().map(|u| u[0]).fold(f32::MAX, f32::min);
        assert!((span - 2.0).abs() < 1e-5);
    }
}
