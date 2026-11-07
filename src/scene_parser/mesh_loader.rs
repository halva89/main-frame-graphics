use crate::render::mesh::Mesh;
use crate::types::Color;
use crate::math::Vec3;

pub struct LoadedMesh {
    pub mesh: Mesh,
    pub position: Vec3,
    pub rotation: Vec3,
    pub scale: Vec3,
}

pub fn load_obj(path: &str) -> Option<LoadedMesh> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut pos_temp: Vec<[f32; 3]> = Vec::new();
    let mut uv_temp: Vec<[f32; 2]> = Vec::new();
    let mut ind_temp: Vec<u32> = Vec::new();
    let mut uv_ind: Vec<u32> = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() { continue; }

        match parts[0] {
            "v" if parts.len() >= 4 => {
                let x: f32 = parts[1].parse().unwrap_or(0.0);
                let y: f32 = parts[2].parse().unwrap_or(0.0);
                let z: f32 = parts[3].parse().unwrap_or(0.0);
                pos_temp.push([x, y, z]);
            }
            "vt" if parts.len() >= 3 => {
                let u: f32 = parts[1].parse().unwrap_or(0.0);
                let v: f32 = parts[2].parse().unwrap_or(0.0);
                uv_temp.push([u, v]);
            }
            "f" => {
                let mut face_indices: Vec<u32> = Vec::new();
                let mut face_uvs: Vec<u32> = Vec::new();
                for i in 1..parts.len() {
                    let sub: Vec<&str> = parts[i].split('/').collect();
                    if let Ok(idx) = sub[0].parse::<u32>() {
                        face_indices.push(idx.wrapping_sub(1));
                    }
                    if sub.len() >= 2 {
                        if let Ok(idx) = sub[1].parse::<u32>() {
                            face_uvs.push(idx.wrapping_sub(1));
                        }
                    }
                }
                if face_indices.len() >= 3 {
                    let first = face_indices[0];
                    for i in 1..face_indices.len() - 1 {
                        ind_temp.push(first);
                        ind_temp.push(face_indices[i]);
                        ind_temp.push(face_indices[i + 1]);
                    }
                }
                if face_uvs.len() >= 3 {
                    let first = face_uvs[0];
                    for i in 1..face_uvs.len() - 1 {
                        uv_ind.push(first);
                        uv_ind.push(face_uvs[i]);
                        uv_ind.push(face_uvs[i + 1]);
                    }
                }
            }
            _ => {}
        }
    }

    if pos_temp.is_empty() || ind_temp.is_empty() {
        println!("[OBJ] Empty or invalid mesh in {}", path);
        return None;
    }

    let has_uv = !uv_temp.is_empty() && uv_ind.len() >= ind_temp.len();
    let index_count = ind_temp.len() as u32;

    let mut vertices = Vec::new();
    for p in &pos_temp {
        vertices.extend_from_slice(p);
    }

    // UV per face-index, not per vertex — need to expand if mesh has uv
    let expand_uvs = if has_uv {
        let mut expanded = vec![0.0f32; pos_temp.len() * 2];
        // Average UV per vertex (not perfect but works for OBJ with shared verts)
        let mut count = vec![0u32; pos_temp.len()];
        for (&pi, &ui) in ind_temp.iter().zip(uv_ind.iter()) {
            if (pi as usize) < pos_temp.len() && (ui as usize) < uv_temp.len() {
                expanded[(pi as usize) * 2] += uv_temp[ui as usize][0];
                expanded[(pi as usize) * 2 + 1] += uv_temp[ui as usize][1];
                count[pi as usize] += 1;
            }
        }
        for i in 0..pos_temp.len() {
            if count[i] > 0 {
                expanded[i * 2] /= count[i] as f32;
                expanded[i * 2 + 1] /= count[i] as f32;
            }
        }
        Some(expanded)
    } else {
        None
    };

    let mut uvs = Vec::new();
    if let Some(e) = expand_uvs {
        uvs = e;
    }

    let normals = Mesh::compute_smooth_normals(&vertices, &ind_temp);

    let mesh = Mesh {
        vertices,
        normals,
        uvs,
        indices: ind_temp,
        color: Color::new(0.7, 0.7, 0.7),
        vertex_count: pos_temp.len() as u32,
        index_count,
        has_uv,
        texture: None,
        texture_mode: crate::render::mesh::TextureMode::Expand,
    };

    println!("[OBJ] Loaded {} with {} vertices, {} triangles{}",
        path, mesh.vertex_count, mesh.index_count / 3,
        if has_uv { " (with UVs)" } else { "" });

    Some(LoadedMesh {
        mesh,
        position: Vec3::zero(),
        rotation: Vec3::zero(),
        scale: Vec3::one(),
    })
}