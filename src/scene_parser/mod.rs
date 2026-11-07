pub mod scene_settings;
pub mod static_objects;
pub mod animated_objects;
pub mod mesh_loader;

pub use scene_settings::{SceneSettings, RenderMode, AaType};
pub use static_objects::parse_static;
pub use animated_objects::parse_animated;

use crate::render::mesh::Mesh;
use crate::animation::AnimatedMesh;
use crate::types::Color;
use crate::math::Vec3;
use crate::physics::PhysicsProps;
use crate::render::mesh::TextureMode;
use std::fs;
use crate::scene_parser::mesh_loader::load_obj;

pub enum SceneObject {
    Static(Mesh, Vec3, Vec3, Vec3, Option<PhysicsProps>, f32),
    Animated(AnimatedMesh, f32),
}

pub fn load_scene(path: &str) -> (SceneSettings, Vec<SceneObject>) {
    println!("[Parser] Loading scene: {}", path);

    let content = fs::read_to_string(path)
        .unwrap_or_else(|_| panic!("Scene file not found: {}", path));

    let lines: Vec<&str> = content.lines().collect();
    println!("[Parser] Read {} lines", lines.len());

    let mut settings = SceneSettings::default();
    let mut objects = Vec::new();
    let mut i = 0;
    let mut pending_pos: Option<Vec3> = None;
    let mut pending_rot: Option<Vec3> = None;
    let mut pending_scale: Option<Vec3> = None;
    let mut pending_phys: Option<PhysicsProps> = None;
    let mut pending_temp: Option<f32> = None;

    while i < lines.len() {
        let line = lines[i].trim();
        println!("[Parser] Line {}: '{}'", i, line);

        if line.is_empty() || line.starts_with('#') {
            i += 1;
            continue;
        }

        if line.starts_with("window") || line.starts_with("title") {
            settings = scene_settings::parse_settings(&lines, &mut i);
            continue;
        }

        if line.starts_with("animate") {
            if let Some(obj) = animated_objects::parse_animated(&lines, &mut i, pending_temp.take().unwrap_or(0.0)) {
                objects.push(obj);
            }
            continue;
        }

        if line.starts_with("position") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                let x = parts[1].parse().unwrap_or(0.0);
                let y = parts[2].parse().unwrap_or(0.0);
                let z = parts[3].parse().unwrap_or(0.0);
                pending_pos = Some(Vec3::new(x, y, z));
            }
            i += 1;
            continue;
        }

        if line.starts_with("rotation") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                let x = parts[1].parse().unwrap_or(0.0);
                let y = parts[2].parse().unwrap_or(0.0);
                let z = parts[3].parse().unwrap_or(0.0);
                pending_rot = Some(Vec3::new(x, y, z));
            }
            i += 1;
            continue;
        }

        if line.starts_with("scale") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                let x = parts[1].parse().unwrap_or(1.0);
                let y = parts[2].parse().unwrap_or(1.0);
                let z = parts[3].parse().unwrap_or(1.0);
                pending_scale = Some(Vec3::new(x, y, z));
            }
            i += 1;
            continue;
        }

        if line.starts_with("physics") {
            let mut p = PhysicsProps::new();
            let val = line.split(':').nth(1).unwrap_or("");
            for part in val.split(',') {
                let kv: Vec<&str> = part.trim().split('=').collect();
                if kv.len() != 2 { continue; }
                let v = kv[1].trim().trim_end_matches('g').trim();
                if let Ok(n) = v.parse::<f32>() {
                    match kv[0].trim() {
                        "m" | "mass" => p.mass = n,
                        "f" | "restitution" | "elasticity" => p.restitution = n,
                        "p" | "density" => p.density = n,
                        _ => {}
                    }
                }
            }
            pending_phys = Some(p);
            i += 1;
            continue;
        }

        if line.starts_with("temp") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                pending_temp = parts[1].parse().ok();
                println!("[Parser] Pending temp: {:?}", pending_temp);
            }
            i += 1;
            continue;
        }

        let pos = pending_pos.take().unwrap_or(Vec3::zero());
        let rot = pending_rot.take().unwrap_or(Vec3::zero());
        let scale = pending_scale.take().unwrap_or(Vec3::one());
        let phys = pending_phys.take();
        let temp = pending_temp.take().unwrap_or(0.0);
        let shape = line;

        if shape == "cube" {
            if i + 2 >= lines.len() { i += 1; continue; }
            let size = lines[i + 1].split_whitespace().next().unwrap_or("1.0").parse().unwrap_or(1.0);
            let color = parse_color(lines[i + 2]);
            let mut mesh = Mesh::cube(size);
            mesh.color = color;
            i += 3;

            // Optional texture lines follow the cube
            if i < lines.len() {
                let tline = lines[i].trim();
                if tline.starts_with("texture") {
                    apply_texture_directive(&mut mesh, tline);
                    i += 1;
                }
            }
            objects.push(SceneObject::Static(mesh, pos, rot, scale, phys, temp));
            continue;
        }

        if shape == "sphere" {
            if i + 3 >= lines.len() { i += 1; continue; }
            let radius = lines[i + 1].split_whitespace().next().unwrap_or("0.5").parse().unwrap_or(0.5);
            let segments = lines[i + 2].split_whitespace().next().unwrap_or("16").parse().unwrap_or(16);
            let color = parse_color(lines[i + 3]);
            let mut mesh = Mesh::sphere(radius, segments);
            mesh.color = color;
            i += 4;

            if i < lines.len() {
                let tline = lines[i].trim();
                if tline.starts_with("texture") {
                    apply_texture_directive(&mut mesh, tline);
                    i += 1;
                }
            }
            objects.push(SceneObject::Static(mesh, pos, rot, scale, phys, temp));
            continue;
        }

        if let Some(obj) = parse_static(line, &lines, &mut i, pos, rot, scale, phys, temp) {
            objects.push(obj);
            continue;
        }

        i += 1;
    }

    println!("[Parser] Loaded {} objects", objects.len());
    (settings, objects)
}

fn parse_color(line: &str) -> Color {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() >= 3 {
        Color::new(
            parts[0].parse().unwrap_or(1.0),
            parts[1].parse().unwrap_or(1.0),
            parts[2].parse().unwrap_or(1.0),
        )
    } else {
        Color::white()
    }
}

fn apply_texture_directive(mesh: &mut Mesh, line: &str) {
    let rest = line.strip_prefix("texture").unwrap_or("").trim();
    let parts: Vec<&str> = rest.split_whitespace().collect();
    if !parts.is_empty() {
        mesh.texture = Some(parts[0].trim_matches('"').to_string());
        for p in &parts[1..] {
            match *p {
                "expand" => mesh.texture_mode = TextureMode::Expand,
                "fill" => mesh.texture_mode = TextureMode::Fill,
                _ => {}
            }
        }
        println!("[Parser] Mesh texture: {:?} mode={:?}", mesh.texture, mesh.texture_mode);
    }
}