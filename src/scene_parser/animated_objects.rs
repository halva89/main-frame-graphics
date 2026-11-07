use crate::animation::AnimatedMesh;
use crate::animation::Keyframe;
use crate::scene_parser::SceneObject;
use crate::types::Color;
use crate::math::Vec3;
use super::parse_color;
use crate::scene_parser::mesh_loader::load_obj;

pub fn parse_animated(lines: &[&str], i: &mut usize, temp: f32) -> Option<SceneObject> {
    let line = lines[*i].trim();

    if !line.starts_with("animate") {
        return None;
    }

    let parts: Vec<&str> = line.split_whitespace().collect();
    let shape_type = if parts.len() >= 2 { parts[1] } else { "triangle" };
    let mesh_path = if shape_type == "mesh" && parts.len() >= 3 {
        Some(parts[2].trim_matches('"').to_string())
    } else {
        None
    };
    // `animate sphere <N>` — число сегментов сферы (по умолчанию 16).
    let sphere_segs: Option<u32> = if shape_type == "sphere" && parts.len() >= 3 {
        parts[2].parse().ok()
    } else {
        None
    };

    let mut keyframes = Vec::new();
    *i += 1;

    while *i < lines.len() {
        let inner = lines[*i].trim();
        if inner == "}" {
            *i += 1;
            break;
        }

        if inner.starts_with("keyframe") {
            let kf_parts: Vec<&str> = inner.split_whitespace().collect();
            if kf_parts.len() >= 2 {
                let time = kf_parts[1].parse::<f32>().unwrap_or(0.0);

                let pos_line = lines[*i + 1].trim();
                let pos_parts: Vec<&str> = pos_line.split_whitespace().collect();
                let pos = if pos_parts.len() >= 2 {
                    let x = pos_parts[0].parse().unwrap_or(0.0);
                    let y = pos_parts[1].parse().unwrap_or(0.0);
                    let z = if pos_parts.len() >= 3 {
                        pos_parts[2].parse().unwrap_or(0.0)
                    } else {
                        0.0
                    };
                    Vec3::new(x, y, z)
                } else {
                    Vec3::zero()
                };

                let color_line = lines[*i + 2].trim();
                let color = parse_color(color_line);

                keyframes.push(Keyframe::new(time, pos, color));
                *i += 3;
            } else {
                *i += 1;
            }
        } else {
            *i += 1;
        }
    }

    if keyframes.is_empty() {
        return None;
    }

    let mut mesh = AnimatedMesh::new();
    mesh.name = format!("anim_{}", shape_type);
    mesh.keyframes = keyframes;
    mesh.duration = mesh.keyframes.last().unwrap().time;

    match shape_type {
        "square" => {
            mesh.vertices = vec![-0.5, -0.5, 0.0, 0.5, -0.5, 0.0, 0.5, 0.5, 0.0, -0.5, 0.5, 0.0];
            mesh.indices = vec![0, 1, 2, 0, 2, 3];
            mesh.normals = vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0];
        }
        "circle" => {
            let segments = 16;
            mesh.vertices.push(0.0);
            mesh.vertices.push(0.0);
            mesh.vertices.push(0.0);
            for j in 0..=segments {
                let angle = (j as f32 / segments as f32) * std::f32::consts::TAU;
                mesh.vertices.push(0.5 * angle.cos());
                mesh.vertices.push(0.5 * angle.sin());
                mesh.vertices.push(0.0);
            }
            for _ in 0..(segments + 2) {
                mesh.normals.push(0.0);
                mesh.normals.push(0.0);
                mesh.normals.push(1.0);
            }
            for j in 0..segments {
                let current = j + 1;
                let next = if j == segments - 1 { 1 } else { j + 2 };
                mesh.indices.push(0);
                mesh.indices.push(current);
                mesh.indices.push(next);
            }
        }
        "cube" => {
            let c = crate::render::mesh::Mesh::cube(1.0);
            mesh.vertices = c.vertices;
            mesh.indices = c.indices;
            mesh.normals = c.normals;
        }
        "sphere" => {
            let m = crate::render::mesh::Mesh::sphere(0.5, sphere_segs.unwrap_or(16));
            mesh.vertices = m.vertices;
            mesh.indices = m.indices;
            mesh.normals = m.normals;
        }
        "mesh" => {
            if let Some(path) = &mesh_path {
                if let Some(loaded) = load_obj(path) {
                    mesh.vertices = loaded.mesh.vertices;
                    mesh.indices = loaded.mesh.indices;
                    mesh.normals = loaded.mesh.normals;
                }
            }
            if mesh.vertices.is_empty() {
                mesh.vertices = vec![-0.5, -0.5, 0.0, 0.5, -0.5, 0.0, 0.0, 0.5, 0.0];
                mesh.indices = vec![0, 1, 2];
                mesh.normals = vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0];
            }
        }
        _ => {
            mesh.vertices = vec![-0.5, -0.5, 0.0, 0.5, -0.5, 0.0, 0.0, 0.5, 0.0];
            mesh.indices = vec![0, 1, 2];
            mesh.normals = vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0];
        }
    }

    Some(SceneObject::Animated(mesh, temp))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_str(s: &str) -> SceneObject {
        let lines: Vec<&str> = s.lines().collect();
        let mut i = 0;
        parse_animated(&lines, &mut i, 0.0).expect("should parse animated object")
    }

    #[test]
    fn test_parse_with_keywords() {
        let obj = parse_str(
            "animate circle\n\
             keyframe 0.0\n\
             pos 0.0 0.0 0.0\n\
             color 1.0 0.0 0.0\n\
             keyframe 2.0\n\
             pos 0.8 0.8 0.0\n\
             color 0.0 1.0 0.0\n\
             keyframe 4.0\n\
             pos 0.0 0.0 0.0\n\
             color 0.0 0.0 1.0",
        );
        let SceneObject::Animated(anim, _) = obj else { panic!("expected animated") };
        assert_eq!(anim.keyframes.len(), 3);
        assert_eq!(anim.keyframes[0].time, 0.0);
        assert_eq!(anim.keyframes[0].position, Vec3::new(0.0, 0.0, 0.0));
        assert_eq!(anim.keyframes[1].position, Vec3::new(0.8, 0.8, 0.0));
        assert_eq!(anim.keyframes[2].color, Color::new(0.0, 0.0, 1.0));
        assert_eq!(anim.duration, 4.0);
        assert_eq!(anim.vertices.len(), 3 * (16 + 2));
    }

    #[test]
    fn test_parse_raw_numbers_2d() {
        let obj = parse_str(
            "animate square\n\
             keyframe 0.0\n\
             0.0 0.0 0.0\n\
             1.0 0.0 0.0\n\
             keyframe 1.0\n\
             1.0 1.0 0.0\n\
             0.0 1.0 0.0",
        );
        let SceneObject::Animated(anim, _) = obj else { panic!("expected animated") };
        assert_eq!(anim.keyframes.len(), 2);
        assert_eq!(anim.keyframes[0].position, Vec3::new(0.0, 0.0, 0.0));
        assert_eq!(anim.keyframes[1].position, Vec3::new(1.0, 1.0, 0.0));
        assert_eq!(anim.keyframes[0].color, Color::new(1.0, 0.0, 0.0));
        assert_eq!(anim.keyframes[1].color, Color::new(0.0, 1.0, 0.0));
        assert!(
            anim.name.starts_with("anim_square_"),
            "name should start with anim_square_, got: {}",
            anim.name
        );
    }

    #[test]
    fn test_unique_names() {
        let obj1 = parse_str("animate circle\nkeyframe 0.0\npos 0 0 0\ncolor 1 1 1");
        let obj2 = parse_str("animate circle\nkeyframe 0.0\npos 0 0 0\ncolor 1 1 1");
        let SceneObject::Animated(a1, _) = obj1 else { panic!() };
        let SceneObject::Animated(a2, _) = obj2 else { panic!() };
        assert_ne!(a1.name, a2.name, "two animations of same shape must have unique names");
    }

    #[test]
    fn test_parse_color_optional() {
        let obj = parse_str("animate triangle\nkeyframe 0.0\npos 1.0 2.0 0.0");
        let SceneObject::Animated(anim, _) = obj else { panic!() };
        assert_eq!(anim.keyframes[0].position, Vec3::new(1.0, 2.0, 0.0));
        assert_eq!(anim.keyframes[0].color, Color::white());
    }

    #[test]
    fn test_animate_cube_is_3d() {
        let obj = parse_str(
            "animate cube\n\
             keyframe 0.0\n\
             pos 0 0 0\n\
             color 1 0 0\n\
             keyframe 1.0\n\
             pos 1 0 0\n\
             color 0 1 0",
        );
        let SceneObject::Animated(anim, _) = obj else { panic!() };
        // Mesh::cube builds 24 vertices (4 per face) * 3 coords = 72 floats
        assert_eq!(anim.vertices.len(), 72, "cube should have 24 vertices x 3 coords");
        assert_eq!(anim.indices.len(), 36, "cube should have 36 indices (6 faces x 2 tris x 3 idx)");
        assert_eq!(anim.normals.len(), 72, "cube should carry per-vertex normals");
    }

    #[test]
    fn test_animate_sphere_is_3d() {
        let obj = parse_str(
            "animate sphere\n\
             keyframe 0.0\n\
             pos 0 0 0\n\
             color 1 1 1",
        );
        let SceneObject::Animated(anim, _) = obj else { panic!() };
        // Sphere with segments=16, rings=8 has (8+1)*(16+1) vertices
        let expected_verts = (8 + 1) * (16 + 1) * 3;
        assert_eq!(anim.vertices.len(), expected_verts, "sphere should have proper 3D vertices");
    }

    #[test]
    fn test_animate_triangle_default_2d() {
        let obj = parse_str(
            "animate unknown\n\
             keyframe 0.0\n\
             pos 0 0 0\n\
             color 1 1 1",
        );
        let SceneObject::Animated(anim, _) = obj else { panic!() };
        // Default fallback is a 2D triangle: 3 vertices x 3 coords
        assert_eq!(anim.vertices.len(), 9, "unknown shape falls back to 2D triangle");
    }
}