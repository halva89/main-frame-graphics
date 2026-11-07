use crate::math::Vec3;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat4 {
    pub data: [[f32; 4]; 4],
}

impl Mat4 {
    pub fn identity() -> Self {
        Self {
            data: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    pub fn zero() -> Self {
        Self {
            data: [[0.0; 4]; 4],
        }
    }

    pub fn translation(x: f32, y: f32, z: f32) -> Self {
        Self {
            data: [
                [1.0, 0.0, 0.0, x],
                [0.0, 1.0, 0.0, y],
                [0.0, 0.0, 1.0, z],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    pub fn scale(sx: f32, sy: f32, sz: f32) -> Self {
        Self {
            data: [
                [sx, 0.0, 0.0, 0.0],
                [0.0, sy, 0.0, 0.0],
                [0.0, 0.0, sz, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    pub fn rotation_x(angle: f32) -> Self {
        let (s, c) = angle.sin_cos();
        Self {
            data: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, c, -s, 0.0],
                [0.0, s, c, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    pub fn rotation_y(angle: f32) -> Self {
        let (s, c) = angle.sin_cos();
        Self {
            data: [
                [c, 0.0, s, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [-s, 0.0, c, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    pub fn rotation_z(angle: f32) -> Self {
        let (s, c) = angle.sin_cos();
        Self {
            data: [
                [c, -s, 0.0, 0.0],
                [s, c, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    pub fn rotation(rx: f32, ry: f32, rz: f32) -> Self {
        let rx_m = Self::rotation_x(rx);
        let ry_m = Self::rotation_y(ry);
        let rz_m = Self::rotation_z(rz);
        Self::multiply(&rz_m, &Self::multiply(&ry_m, &rx_m))
    }

    pub fn multiply(a: &Self, b: &Self) -> Self {
        let mut result = Self::zero();

        for i in 0..4 {
            for j in 0..4 {
                let mut sum = 0.0;
                for k in 0..4 {
                    sum += a.data[i][k] * b.data[k][j];
                }
                result.data[i][j] = sum;
            }
        }

        result
    }

    pub fn transform(&self, v: &Vec3) -> Vec3 {
        let x = self.data[0][0] * v.x + self.data[0][1] * v.y + self.data[0][2] * v.z + self.data[0][3];
        let y = self.data[1][0] * v.x + self.data[1][1] * v.y + self.data[1][2] * v.z + self.data[1][3];
        let z = self.data[2][0] * v.x + self.data[2][1] * v.y + self.data[2][2] * v.z + self.data[2][3];
        Vec3::new(x, y, z)
    }

    pub fn as_slice(&self) -> &[f32; 16] {
        unsafe { &*(self.data.as_ptr() as *const [f32; 16]) }
    }

    // Ортографическая проекция для 2D
    pub fn orthographic(left: f32, right: f32, bottom: f32, top: f32, near: f32, far: f32) -> Self {
        Self {
            data: [
                [2.0 / (right - left), 0.0, 0.0, -(right + left) / (right - left)],
                [0.0, 2.0 / (top - bottom), 0.0, -(top + bottom) / (top - bottom)],
                // Vulkan: глубина в диапазоне 0..1 (не -1..1, как в OpenGL).
                [0.0, 0.0, -1.0 / (far - near), -near / (far - near)],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    // Перспективная проекция для Vulkan (z range: 0..1, а не -1..1)
    pub fn perspective(fov: f32, aspect: f32, near: f32, far: f32) -> Self {
        let tan_half_fov = (fov / 2.0).tan();
        let mut result = Self::zero();
        result.data[0][0] = 1.0 / (aspect * tan_half_fov);
        // Vulkan: NDC Y направлен вниз (в отличие от OpenGL), поэтому ось Y
        // инвертируется здесь, а не во вьюпорте. Матрица используется и для
        // пасса теней, так что карта теней остаётся согласованной с выборкой.
        result.data[1][1] = -1.0 / tan_half_fov;
        result.data[2][2] = far / (near - far);
        result.data[2][3] = -(far * near) / (far - near);
        result.data[3][2] = -1.0;
        result
    }

    // Матрица вида (камера)
    pub fn look_at(eye: Vec3, target: Vec3, up: Vec3) -> Self {
        let forward = (target - eye).normalize();
        let right = forward.cross(&up).normalize();
        let up = right.cross(&forward);

        let mut result = Self::identity();
        result.data[0][0] = right.x;
        result.data[0][1] = right.y;
        result.data[0][2] = right.z;
        result.data[1][0] = up.x;
        result.data[1][1] = up.y;
        result.data[1][2] = up.z;
        result.data[2][0] = -forward.x;
        result.data[2][1] = -forward.y;
        result.data[2][2] = -forward.z;

        let translation = Mat4::translation(-eye.x, -eye.y, -eye.z);
        Self::multiply(&result, &translation)
    }

    pub fn transpose(&self) -> Self {
        let mut result = Self::zero();
        for i in 0..4 {
            for j in 0..4 {
                result.data[i][j] = self.data[j][i];
            }
        }
        result
    }
}

impl Default for Mat4 {
    fn default() -> Self {
        Self::identity()
    }
}