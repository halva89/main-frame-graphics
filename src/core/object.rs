use crate::core::transform::Transform;
use crate::render::mesh::Mesh;
use crate::scene_parser::SceneObject;
use crate::types::Color;
use crate::physics::PhysicsProps;
use crate::math::Vec3;
use rapier3d::dynamics::RigidBodyHandle;

// Локальный (до трансформации) AABB меша. Пересчитывается по vertices один раз,
// при создании объекта; в мировые координаты переводится каждый кадр в рендере.
pub fn mesh_aabb(vertices: &[f32]) -> (Vec3, Vec3) {
    let mut min = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
    let mut max = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
    if !vertices.is_empty() {
        for v in vertices.chunks(3) {
            if v.len() < 3 { break; }
            min.x = min.x.min(v[0]); min.y = min.y.min(v[1]); min.z = min.z.min(v[2]);
            max.x = max.x.max(v[0]); max.y = max.y.max(v[1]); max.z = max.z.max(v[2]);
        }
    } else {
        min = Vec3::new(-0.5, -0.5, -0.5);
        max = Vec3::new(0.5, 0.5, 0.5);
    }
    (min, max)
}

pub struct Object {
    pub mesh: Mesh,
    pub transform: Transform,
    pub is_animated: bool,
    pub color: Color,
    pub name: String,
    pub anim_name: String,
    pub physics: Option<PhysicsProps>,
    pub physics_handle: Option<RigidBodyHandle>,
    // Локальный AABB для culling'а (view-frustum + occlusion)
    pub aabb_min: Vec3,
    pub aabb_max: Vec3,
    // Температура объекта (для теплового режима)
    pub temp: f32,
}

impl Object {
    pub fn new(mesh: Mesh, transform: Transform) -> Self {
        let (aabb_min, aabb_max) = mesh_aabb(&mesh.vertices);
        Self {
            mesh,
            transform,
            is_animated: false,
            color: Color::white(),
            name: String::from("Object"),
            anim_name: String::new(),
            physics: None,
            physics_handle: None,
            aabb_min,
            aabb_max,
            temp: 0.0,
        }
    }

    pub fn from(scene_obj: SceneObject) -> Self {
        match scene_obj {
            SceneObject::Static(mesh, position, rotation, scale, phys, temp) => {
                let color = mesh.color;
                let mut transform = Transform::default();
                transform.set_position(position.x, position.y, position.z);
                transform.set_rotation(rotation.x, rotation.y, rotation.z);
                transform.set_scale(scale.x, scale.y, scale.z);
                let (aabb_min, aabb_max) = mesh_aabb(&mesh.vertices);
                Self {
                    mesh,
                    transform,
                    is_animated: false,
                    color,
                    name: String::from("Static"),
                    anim_name: String::new(),
                    physics: phys,
                    physics_handle: None,
                    aabb_min,
                    aabb_max,
                    temp,
                }
            }
            SceneObject::Animated(animated_mesh, temp) => {
                let color = if !animated_mesh.keyframes.is_empty() {
                    animated_mesh.keyframes[0].color
                } else {
                    Color::white()
                };
                let anim_name = animated_mesh.name.clone();
                let mesh = Mesh::from_animated(animated_mesh);
                let (aabb_min, aabb_max) = mesh_aabb(&mesh.vertices);
                Self {
                    mesh,
                    transform: Transform::default(),
                    is_animated: true,
                    color,
                    name: String::from("Animated"),
                    anim_name,
                    physics: None,
                    physics_handle: None,
                    aabb_min,
                    aabb_max,
                    temp,
                }
            }
        }
    }

    pub fn set_position(&mut self, x: f32, y: f32, z: f32) {
        self.transform.set_position(x, y, z);
    }

    pub fn set_scale(&mut self, x: f32, y: f32, z: f32) {
        self.transform.set_scale(x, y, z);
    }

    pub fn set_rotation(&mut self, x: f32, y: f32, z: f32) {
        self.transform.set_rotation(x, y, z);
    }
}