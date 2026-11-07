use crate::types::Color;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RenderMode {
    Light,
    Heavy,
}

impl Default for RenderMode {
    fn default() -> Self {
        Self::Light
    }
}

// Антиалиасинг. None — без сглаживания, Msaa2/Msaa4 — мультисэмплинг,
// Fxaa — пост-обработка (дёшево, работает на всём кадре).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AaType {
    None,
    Msaa2,
    Msaa4,
    Fxaa,
}

impl Default for AaType {
    fn default() -> Self {
        Self::None
    }
}

impl AaType {
    pub fn sample_count(&self) -> u32 {
        match self {
            AaType::Msaa2 => 2,
            AaType::Msaa4 => 4,
            _ => 1,
        }
    }
}

pub struct SceneSettings {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub render_mode: RenderMode,
    pub aa_type: AaType,
    pub camera_enabled: bool,
    pub camera_pos: (f32, f32, f32),
    pub camera_target: (f32, f32, f32),
    pub decorated: bool,
    pub resizable: bool,
    pub wireframe: bool,
    pub texture: Option<String>,
    pub background: Color,
    pub thermal: bool,
}

impl Default for SceneSettings {
    fn default() -> Self {
        Self {
            title: "main.frame".to_string(),
            width: 800,
            height: 600,
            render_mode: RenderMode::Light,
            aa_type: AaType::None,
            camera_enabled: true,
            camera_pos: (0.0, 0.0, 5.0),
            camera_target: (0.0, 0.0, 0.0),
            decorated: true,
            resizable: true,
            wireframe: false,
            texture: None,
            background: Color::new(0.63, 0.91, 1.0),
            thermal: false,
        }
    }
}

pub fn parse_settings(lines: &[&str], i: &mut usize) -> SceneSettings {
    let mut settings = SceneSettings::default();
    println!("[SettingsParser] Parsing settings...");

    while *i < lines.len() {
        let line = lines[*i].trim();
        println!("[SettingsParser] Line: '{}'", line);

        if line.is_empty() || line.starts_with('#') {
            *i += 1;
            continue;
        }

        if line.starts_with("window") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                if let (Ok(w), Ok(h)) = (parts[1].parse(), parts[2].parse()) {
                    settings.width = w;
                    settings.height = h;
                    println!("[SettingsParser] Window: {}x{}", w, h);
                }
            }
            *i += 1;
            continue;
        }

        if line.starts_with("title") {
            let title_part = line.strip_prefix("title").unwrap_or("").trim();
            let title = title_part
                .trim_start_matches('"')
                .trim_end_matches('"')
                .to_string();
            settings.title = title;
            println!("[SettingsParser] Title: '{}'", settings.title);
            *i += 1;
            continue;
        }

        if line.starts_with("aa-type") || line.starts_with("aa_type") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                match parts[1].to_lowercase().as_str() {
                    "none" | "off" | "0" => settings.aa_type = AaType::None,
                    "msaa2" | "2x" => settings.aa_type = AaType::Msaa2,
                    "msaa4" | "4x" | "msaa4w" => settings.aa_type = AaType::Msaa4,
                    "fxaa" => settings.aa_type = AaType::Fxaa,
                    _ => settings.aa_type = AaType::None,
                }
                println!("[SettingsParser] AA type: {:?}", settings.aa_type);
            }
            *i += 1;
            continue;
        }

        if line.starts_with("render_mode") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                match parts[1] {
                    "heavy" | "Heavy" | "HEAVY" => {
                        settings.render_mode = RenderMode::Heavy;
                        println!("[SettingsParser] Render mode: Heavy (discrete GPU)");
                    }
                    _ => {
                        settings.render_mode = RenderMode::Light;
                        println!("[SettingsParser] Render mode: Light (integrated GPU)");
                    }
                }
            }
            *i += 1;
            continue;
        }

        if line.starts_with("camera_pos") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                settings.camera_pos = (
                    parts[1].parse().unwrap_or(0.0),
                    parts[2].parse().unwrap_or(0.0),
                    parts[3].parse().unwrap_or(5.0),
                );
                println!("[SettingsParser] Camera pos: {:?}", settings.camera_pos);
            }
            *i += 1;
            continue;
        }

        if line.starts_with("camera_target") || line.starts_with("camera_look") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                settings.camera_target = (
                    parts[1].parse().unwrap_or(0.0),
                    parts[2].parse().unwrap_or(0.0),
                    parts[3].parse().unwrap_or(0.0),
                );
                println!("[SettingsParser] Camera target: {:?}", settings.camera_target);
            }
            *i += 1;
            continue;
        }

        if line.starts_with("camera") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                settings.camera_enabled = parts[1] == "true" || parts[1] == "1" || parts[1] == "on";
                println!("[SettingsParser] Camera enabled: {}", settings.camera_enabled);
            }
            *i += 1;
            continue;
        }

        if line.starts_with("thermal") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                settings.thermal = parts[1] == "true" || parts[1] == "1" || parts[1] == "on";
                println!("[SettingsParser] Thermal mode: {}", settings.thermal);
            }
            *i += 1;
            continue;
        }

if line.starts_with("decorated") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                settings.decorated = parts[1] == "true" || parts[1] == "1" || parts[1] == "on";
                println!("[SettingsParser] Decorated: {}", settings.decorated);
            }
            *i += 1;
            continue;
        }

        if line.starts_with("resizable") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                settings.resizable = parts[1] == "true" || parts[1] == "1" || parts[1] == "on";
                println!("[SettingsParser] Resizable: {}", settings.resizable);
            }
            *i += 1;
            continue;
        }

        if line.starts_with("wireframe") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                settings.wireframe = parts[1] == "true" || parts[1] == "1" || parts[1] == "on";
                println!("[SettingsParser] Wireframe: {}", settings.wireframe);
            }
            *i += 1;
            continue;
        }

        if line.starts_with("texture") {
            let tex = line.strip_prefix("texture").unwrap_or("").trim().trim_matches('"').to_string();
            if !tex.is_empty() {
                println!("[SettingsParser] Texture: '{}'", tex);
                settings.texture = Some(tex);
            }
            *i += 1;
            continue;
        }

        if line.starts_with("bg") || line.starts_with("background") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                let r = parts[1].parse().unwrap_or(0.63);
                let g = parts[2].parse().unwrap_or(0.91);
                let b = parts[3].parse().unwrap_or(1.0);
                settings.background = Color::new(r, g, b);
                println!("[SettingsParser] Background: {} {} {}", r, g, b);
            }
            *i += 1;
            continue;
        }

        if line == "triangle" || line == "square" || line == "circle" || line == "cube" || line == "sphere" || line == "mesh" || line.starts_with("position") || line.starts_with("animate") {
            println!("[SettingsParser] Found shape, stopping settings parse");
            break;
        }

        *i += 1;
    }

    settings
}