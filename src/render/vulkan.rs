use ash::{vk, Entry, Instance, Device};
use ash::khr::{surface, swapchain};
use crate::platform::CustomWindow;
use crate::render::Pipeline;
use crate::render::pipeline::samples_flags;
use crate::render::Uniforms;
use crate::core::scene::Scene;
use crate::math::{Mat4, Vec3};
use std::ffi::CString;
use std::collections::HashMap;
use crate::render::mesh::TextureMode;
use crate::scene_parser::AaType;

// Матрица нормалей: inverse(transpose(mat3(model))). Перенос игнорируем.
fn normal_transform(m: &Mat4) -> [[f32; 3]; 3] {
    let a00 = m.data[0][0]; let a01 = m.data[0][1]; let a02 = m.data[0][2];
    let a10 = m.data[1][0]; let a11 = m.data[1][1]; let a12 = m.data[1][2];
    let a20 = m.data[2][0]; let a21 = m.data[2][1]; let a22 = m.data[2][2];
    let det = a00 * (a11 * a22 - a12 * a21) - a01 * (a10 * a22 - a12 * a20) + a02 * (a10 * a21 - a11 * a20);
    let inv = if det.abs() < 1e-12 { 1.0 } else { 1.0 / det };
    let n00 = (a11 * a22 - a12 * a21) * inv;
    let n01 = -(a10 * a22 - a12 * a20) * inv;
    let n02 = (a10 * a21 - a11 * a20) * inv;
    let n10 = -(a01 * a22 - a02 * a21) * inv;
    let n11 = (a00 * a22 - a02 * a20) * inv;
    let n12 = -(a00 * a21 - a01 * a20) * inv;
    let n20 = (a01 * a12 - a02 * a11) * inv;
    let n21 = -(a00 * a12 - a02 * a10) * inv;
    let n22 = (a00 * a11 - a01 * a10) * inv;
    [[n00, n01, n02], [n10, n11, n12], [n20, n21, n22]]
}

fn apply_normal(n: &[f32; 3], nt: &[[f32; 3]; 3]) -> [f32; 3] {
    let nx = nt[0][0] * n[0] + nt[0][1] * n[1] + nt[0][2] * n[2];
    let ny = nt[1][0] * n[0] + nt[1][1] * n[1] + nt[1][2] * n[2];
    let nz = nt[2][0] * n[0] + nt[2][1] * n[1] + nt[2][2] * n[2];
    let len = (nx * nx + ny * ny + nz * nz).sqrt();
    if len > 1e-8 { [nx / len, ny / len, nz / len] } else { [0.0, 0.0, 1.0] }
}

// ---- View-frustum culling helpers ----

// Температура → цвет (тепловизор): холодный=тёмно-синий, тёплый=красный,
// горячий=жёлтый, очень горячий=белый. t нормализуется между min_t и max_t.
fn temp_to_color(t: f32, min_t: f32, max_t: f32) -> [f32; 3] {
    let n = if max_t > min_t { ((t - min_t) / (max_t - min_t)).clamp(0.0, 1.0) } else { 0.0 };
    const STOPS: [(f32, [f32; 3]); 4] = [
        (0.00, [0.00, 0.00, 0.55]),
        (0.33, [1.00, 0.00, 0.00]),
        (0.66, [1.00, 1.00, 0.00]),
        (1.00, [1.00, 1.00, 1.00]),
    ];
    if n <= STOPS[0].0 { return STOPS[0].1; }
    for pair in STOPS.windows(2) {
        let (a, ca) = pair[0];
        let (b, cb) = pair[1];
        if n <= b {
            let u = (n - a) / (b - a);
            return [
                ca[0] + (cb[0] - ca[0]) * u,
                ca[1] + (cb[1] - ca[1]) * u,
                ca[2] + (cb[2] - ca[2]) * u,
            ];
        }
    }
    STOPS[3].1
}

// Шесть плоскостей фрустума из view-proj (Gribb-Hartmann), нормализованные.
fn frustum_planes(vp: &Mat4) -> [[f32; 4]; 6] {
    let m = &vp.data;
    let r0 = [m[0][0], m[0][1], m[0][2], m[0][3]];
    let r1 = [m[1][0], m[1][1], m[1][2], m[1][3]];
    let r2 = [m[2][0], m[2][1], m[2][2], m[2][3]];
    let r3 = [m[3][0], m[3][1], m[3][2], m[3][3]];
    let defs: [([f32; 4], [f32; 4], bool); 6] = [
        (r3, r0, false), (r3, r0, true),
        (r3, r1, false), (r3, r1, true),
        (r3, r2, false), (r3, r2, true),
    ];
    let mut out = [[0.0f32; 4]; 6];
    for (k, (a_, b_, sub)) in defs.iter().enumerate() {
        let s = if *sub { -1.0f32 } else { 1.0f32 };
        let p = [a_[0] + s * b_[0], a_[1] + s * b_[1], a_[2] + s * b_[2], a_[3] + s * b_[3]];
        let len = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
        let inv = if len > 1e-12 { 1.0 / len } else { 1.0 };
        out[k] = [p[0] * inv, p[1] * inv, p[2] * inv, p[3] * inv];
    }
    out
}

// true, если AABB целиком снаружи фрустума → можно не рисовать.
fn aabb_outside_frustum(min: &[f32; 3], max: &[f32; 3], planes: &[[f32; 4]; 6]) -> bool {
    for p in planes {
        let (a, b, c, d) = (p[0], p[1], p[2], p[3]);
        let vx = if a > 0.0 { max[0] } else { min[0] };
        let vy = if b > 0.0 { max[1] } else { min[1] };
        let vz = if c > 0.0 { max[2] } else { min[2] };
        if a * vx + b * vy + c * vz + d < 0.0 { return true; }
    }
    false
}

fn world_aabb(min: Vec3, max: Vec3, m: &Mat4) -> ([f32; 3], [f32; 3]) {
    let cs = [
        Vec3::new(min.x, min.y, min.z), Vec3::new(max.x, min.y, min.z),
        Vec3::new(min.x, max.y, min.z), Vec3::new(min.x, min.y, max.z),
        Vec3::new(max.x, max.y, min.z), Vec3::new(max.x, min.y, max.z),
        Vec3::new(min.x, max.y, max.z), Vec3::new(max.x, max.y, max.z),
    ];
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for p in cs {
        let q = m.transform(&p);
        lo[0] = lo[0].min(q.x); lo[1] = lo[1].min(q.y); lo[2] = lo[2].min(q.z);
        hi[0] = hi[0].max(q.x); hi[1] = hi[1].max(q.y); hi[2] = hi[2].max(q.z);
    }
    (lo, hi)
}

struct TexEntry {
    image: vk::Image,
    memory: vk::DeviceMemory,
    view: vk::ImageView,
    sampler: vk::Sampler,
    width: u32,
    height: u32,
}

pub struct VulkanContext {
    pub entry: Entry,
    pub instance: Instance,
    pub surface: vk::SurfaceKHR,
    pub surface_loader: surface::Instance,
    pub device: Device,
    pub physical_device: vk::PhysicalDevice,
    pub queue_family_index: u32,
    pub queue: vk::Queue,
    pub swapchain_loader: swapchain::Device,
    pub swapchain: vk::SwapchainKHR,
    pub swapchain_images: Vec<vk::Image>,
    pub swapchain_image_views: Vec<vk::ImageView>,
    pub swapchain_format: vk::Format,
    pub swapchain_extent: vk::Extent2D,
    pub render_pass: vk::RenderPass,
    pub pipeline: Pipeline,
    pub pipeline_layout: vk::PipelineLayout,
    pub descriptor_set_layout: vk::DescriptorSetLayout,
    pub descriptor_pool: vk::DescriptorPool,
    pub descriptor_set_scene: vk::DescriptorSet,
    pub descriptor_set_ov: vk::DescriptorSet,
    pub descriptor_set_shadow: vk::DescriptorSet,
    pub framebuffers: Vec<vk::Framebuffer>,
    pub command_pool: vk::CommandPool,
    pub command_buffers: Vec<vk::CommandBuffer>,
    pub image_available_semaphore: vk::Semaphore,
    pub render_finished_semaphore: vk::Semaphore, pub fence: vk::Fence,
    pub vertex_buffer: vk::Buffer, pub vertex_buffer_memory: vk::DeviceMemory,
    pub index_buffer: vk::Buffer, pub index_buffer_memory: vk::DeviceMemory,
    pub edge_vertex_buffer: vk::Buffer, pub edge_vertex_buffer_memory: vk::DeviceMemory,
    pub edge_index_buffer: vk::Buffer, pub edge_index_buffer_memory: vk::DeviceMemory,
    pub uniform_buffer: vk::Buffer, pub uniform_buffer_memory: vk::DeviceMemory,
    pub uniform_buffer_ov: vk::Buffer, pub uniform_buffer_ov_memory: vk::DeviceMemory,
    pub shadow_uniform_buffer: vk::Buffer, pub shadow_uniform_buffer_memory: vk::DeviceMemory,
    pub vertex_count: u32, pub index_count: u32,
    pub edge_vertex_count: u32, pub edge_index_count: u32,
    pub current_buffer_size: u64, pub current_index_buffer_size: u64,
    pub current_edge_buffer_size: u64, pub current_edge_index_buffer_size: u64,
    pub line_width: f32,
    pub uniforms: Uniforms, pub uniforms_ov: Uniforms,
    pub eye: Vec3, pub target: Vec3,
    pub depth_format: vk::Format,
    pub depth_image: vk::Image,
    pub depth_image_memory: vk::DeviceMemory,
    pub depth_image_view: vk::ImageView,
    pub shadow_render_pass: vk::RenderPass,
    pub shadow_map_size: u32,
    pub shadow_map: vk::Image, pub shadow_map_memory: vk::DeviceMemory,
    pub shadow_map_view: vk::ImageView,
    pub shadow_map_sampler: vk::Sampler,
    pub shadow_framebuffer: vk::Framebuffer,
    pub shadow_color: vk::Image, pub shadow_color_memory: vk::DeviceMemory,
    pub shadow_color_view: vk::ImageView,
    pub shadow_bias: f32,
    pub texture_image: vk::Image, pub texture_image_memory: vk::DeviceMemory,
    pub texture_image_view: vk::ImageView,
    pub texture_sampler: vk::Sampler,
    pub use_texture: bool,
    pub textures: HashMap<String, (TexEntry, vk::DescriptorSet)>,
    pub background: [f32; 3],
    pub aa: AaType, pub aa_samples: u32, pub use_fxaa: bool,
    // MSAA (только когда aa_samples > 1)
    pub msaa_color: vk::Image, pub msaa_color_view: vk::ImageView, pub msaa_color_mem: vk::DeviceMemory,
    pub msaa_depth: vk::Image, pub msaa_depth_view: vk::ImageView, pub msaa_depth_mem: vk::DeviceMemory,
    // FXAA (только когда use_fxaa)
    pub scene_color: Option<vk::Image>, pub scene_color_view: Option<vk::ImageView>, pub scene_color_mem: Option<vk::DeviceMemory>,
    pub present_render_pass: Option<vk::RenderPass>,
    pub present_framebuffers: Vec<vk::Framebuffer>,
    pub present_pipeline: Option<vk::Pipeline>,
    pub present_layout: Option<vk::PipelineLayout>,
    pub present_dsl: Option<vk::DescriptorSetLayout>,
    pub present_set: Option<vk::DescriptorSet>,
    pub present_sampler: Option<vk::Sampler>,
    pub present_vb: vk::Buffer, pub present_vb_mem: vk::DeviceMemory,
    current_image_index: u32, window_width: u32, window_height: u32, wireframe: bool,
    thermal_mode: bool,
    additive_pipeline: vk::Pipeline,
    ui_pipeline: vk::Pipeline,
    glow_tex: Option<(vk::Image, vk::DeviceMemory, vk::ImageView, vk::Sampler, vk::DescriptorSet)>,
    glow_debug_done: bool,
    heat_state: Vec<f32>,
}

impl VulkanContext {
    pub fn new(window: &CustomWindow, prefer_discrete: bool, aa: AaType, thermal: bool) -> Self {
        println!("[Vulkan] Starting init... AA = {:?}", aa);
        let entry = unsafe { Entry::load() }.unwrap();
        let app_name = CString::new("Snow").unwrap();
        let app_info = vk::ApplicationInfo::default()
            .application_name(&app_name)
            .application_version(vk::make_api_version(0, 1, 0, 0))
            .api_version(vk::make_api_version(0, 1, 0, 0));

        let display_handle = window.get_raw_display_handle();
        let extension_names = ash_window::enumerate_required_extensions(display_handle)
            .expect("Failed to get extensions")
            .to_vec();

        let instance_create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_extension_names(&extension_names);

        let instance = unsafe { entry.create_instance(&instance_create_info, None) }.unwrap();

        let surface_loader = surface::Instance::new(&entry, &instance);
        let surface = unsafe {
            ash_window::create_surface(&entry, &instance, display_handle, window.get_raw_window_handle(), None)
        }.unwrap();

        let pds = unsafe { instance.enumerate_physical_devices() }.unwrap();
        let has_discrete = pds.iter().any(|&d| {
            unsafe { instance.get_physical_device_properties(d) }.device_type == vk::PhysicalDeviceType::DISCRETE_GPU
        });
        let has_integrated = pds.iter().any(|&d| {
            unsafe { instance.get_physical_device_properties(d) }.device_type == vk::PhysicalDeviceType::INTEGRATED_GPU
        });

        let pd = if prefer_discrete && has_discrete {
            // Heavy mode: prefer discrete GPU
            pds.iter()
                .find(|&&d| unsafe { instance.get_physical_device_properties(d) }.device_type == vk::PhysicalDeviceType::DISCRETE_GPU)
                .copied()
                .unwrap_or(pds[0])
        } else if !prefer_discrete && has_integrated {
            *pds.iter().find(|&&d| unsafe { instance.get_physical_device_properties(d) }.device_type == vk::PhysicalDeviceType::INTEGRATED_GPU).unwrap_or(&pds[0])
        } else { pds[0] };
        let sfl = surface_loader;
        let pname = unsafe { instance.get_physical_device_properties(pd) }.device_name;
        let cstr = unsafe { std::ffi::CStr::from_ptr(pname.as_ptr()) }.to_string_lossy().into_owned();
        println!("[Vulkan] Device: {}", cstr);
        let qfi = Self::find_qf(&instance, pd, surface, &sfl).unwrap();
        let dev_ext = [CString::new("VK_KHR_swapchain").unwrap()];
        let ext_ptrs: Vec<*const i8> = dev_ext.iter().map(|c| c.as_ptr()).collect();

        let qp = 1.0f32;
        let qci = vk::DeviceQueueCreateInfo::default().queue_family_index(qfi).queue_priorities(std::slice::from_ref(&qp));
        let dci = vk::DeviceCreateInfo::default().queue_create_infos(std::slice::from_ref(&qci)).enabled_extension_names(&ext_ptrs);
        let device = unsafe { instance.create_device(pd, &dci, None) }.unwrap();
        let queue = unsafe { device.get_device_queue(qfi, 0) };

        let sl = swapchain::Device::new(&instance, &device);
        let (sc, fmt, ext, imgs) = match Self::create_swapchain(pd, &sl, surface, &sfl, window.width, window.height) {
            Some(v) => v, None => { panic!("Failed to create swapchain"); }
        };
        let ivs: Vec<_> = imgs.iter().map(|&img| unsafe { device.create_image_view(
            &vk::ImageViewCreateInfo::default().image(img).view_type(vk::ImageViewType::TYPE_2D).format(fmt)
            .subresource_range(vk::ImageSubresourceRange::default().aspect_mask(vk::ImageAspectFlags::COLOR).base_mip_level(0).level_count(1).base_array_layer(0).layer_count(1)), None) }.unwrap()).collect();
        let (dimg, dimg_mem, dimg_view, dfmt) = Self::create_depth(&device, &instance, pd, ext.width, ext.height, 1);
        // ===== Антиалиасинг: определяем сэмплы =====
        let mut samples = aa.sample_count();
        let pprops = unsafe { instance.get_physical_device_properties(pd) };
        loop {
            let sf = samples_flags(samples);
            let cok = (pprops.limits.framebuffer_color_sample_counts & sf) == sf;
            let dok = (pprops.limits.framebuffer_depth_sample_counts & sf) == sf;
            if samples <= 1 || (cok && dok) { break; }
            samples = if samples >= 4 { 2 } else { 1 };
        }
        println!("[Vulkan] AA = {:?} → samples = {}", aa, samples);
        let msaa = samples > 1;
        let use_fxaa = !msaa && aa == AaType::Fxaa;
        let sm_size = 1024u32;
        let (sm_map, sm_mem, sm_view, sm_sampler) = Self::create_shadow_map(&device, &instance, pd, sm_size);
        // Фиктивный цветовой аттачмент для пасса теней: некоторые драйверы
        // (в т.ч. программные) отказывают в создании пайплайна для подпасса
        // вообще без цветовых аттачментов. Цвет не читаем и не пишем.
        let (sm_col, sm_col_mem, sm_col_view) = Self::create_color_target(&device, &instance, pd, sm_size);
        let smrp = Self::create_shadow_rp(&device, vk::Format::R8G8B8A8_UNORM, vk::Format::D32_SFLOAT);
        let smfb = unsafe { device.create_framebuffer(&vk::FramebufferCreateInfo::default().render_pass(smrp).attachments(&[sm_col_view, sm_view]).width(sm_size).height(sm_size).layers(1), None) }.unwrap();
        // MSAA / offscreen-цели
        let mut msaa_col = vk::Image::null(); let mut msaa_col_mem = vk::DeviceMemory::null(); let mut msaa_col_view = vk::ImageView::null();
        let mut msaa_dimg = vk::Image::null(); let mut msaa_dimg_mem = vk::DeviceMemory::null(); let mut msaa_dimg_view = vk::ImageView::null();
        let mut sc_col = vk::Image::null(); let mut sc_col_mem = vk::DeviceMemory::null(); let mut sc_col_view = vk::ImageView::null();
        let rp = if msaa {
            let (c, cm, cv) = Self::create_color_samples(&device, &instance, pd, fmt, ext.width, ext.height, samples, false);
            msaa_col = c; msaa_col_mem = cm; msaa_col_view = cv;
            let (d, dm, dv, _) = Self::create_depth(&device, &instance, pd, ext.width, ext.height, samples);
            msaa_dimg = d; msaa_dimg_mem = dm; msaa_dimg_view = dv;
            Self::create_msaa_rp(&device, fmt, dfmt, samples)
        } else if use_fxaa {
            let (c, cm, cv) = Self::create_color_samples(&device, &instance, pd, fmt, ext.width, ext.height, 1, true);
            sc_col = c; sc_col_mem = cm; sc_col_view = cv;
            Self::create_rp_fxaa(&device, fmt, dfmt)
        } else {
            Self::create_rp(&device, fmt, dfmt)
        };
        let (dsl, dpool, dset, dset2, dset_shadow) = Self::create_desc(&device);
        let pll = unsafe { device.create_pipeline_layout(&vk::PipelineLayoutCreateInfo::default().set_layouts(std::slice::from_ref(&dsl)), None) }.unwrap();
        let mut pipeline = Pipeline::new_with_layout(&device, rp, ext, pll, 2.0, samples);
        pipeline.shadow_handle = Pipeline::create_shadow(&device, smrp, pll);
        let fbs: Vec<_> = if msaa {
            ivs.iter().map(|&view| {
                let att = [msaa_col_view, view, msaa_dimg_view];
                unsafe { device.create_framebuffer(&vk::FramebufferCreateInfo::default().render_pass(rp).attachments(&att).width(ext.width).height(ext.height).layers(1), None) }.unwrap()
            }).collect()
        } else if use_fxaa {
            let att = [sc_col_view, dimg_view];
            let fb = unsafe { device.create_framebuffer(&vk::FramebufferCreateInfo::default().render_pass(rp).attachments(&att).width(ext.width).height(ext.height).layers(1), None) }.unwrap();
            (0..ivs.len()).map(|_| fb).collect()
        } else {
            ivs.iter().map(|&view| {
                let att = [view, dimg_view];
                unsafe { device.create_framebuffer(&vk::FramebufferCreateInfo::default().render_pass(rp).attachments(&att).width(ext.width).height(ext.height).layers(1), None) }.unwrap()
            }).collect()
        };
        let cpool = unsafe { device.create_command_pool(&vk::CommandPoolCreateInfo::default().queue_family_index(qfi), None) }.unwrap();
        let cbs = unsafe { device.allocate_command_buffers(&vk::CommandBufferAllocateInfo::default().command_pool(cpool).level(vk::CommandBufferLevel::PRIMARY).command_buffer_count(fbs.len() as u32)) }.unwrap();
        let dummies: Vec<f32> = vec![0.0; 11];
        let dummyi: Vec<u32> = vec![0, 1, 2];
        let (vb, vbm, vc) = Self::create_vb(&device, &instance, pd, &dummies);
        let (ib, ibm, ic) = Self::create_ib(&device, &instance, pd, &dummyi);
        let aspect = ext.width as f32 / ext.height as f32;
        let mut ubo = Uniforms::new();
        let aspect = ext.width as f32 / ext.height as f32;
        ubo.projection = Mat4::perspective(45.0_f32.to_radians(), aspect, 0.1, 100.0);
        let (ub, ubm) = Self::create_ub(&device, &instance, pd, &ubo);
        let mut ubo_ov = Uniforms::new();
        ubo_ov.projection = Mat4::orthographic(-1.0, 1.0, -1.0, 1.0, 0.0, 10.0);
        let (ub2, ubm2) = Self::create_ub(&device, &instance, pd, &ubo_ov);
        // Отдельный UBO для пасса карты теней: те же данные, но shadow_params.w = 1,
        // чтобы основной шейдер проецировал геометрию в пространство света.
        let (ub_shadow, ubm_shadow) = Self::create_ub(&device, &instance, pd, &ubo);

        // Аддитивный пайплайн для сияния (тепловизор): глоу-квады как billboard.
        let ui_pipeline = Pipeline::create_ui(&device, rp, pll, samples);
        let additive_handle = Self::create_additive_pipeline(&device, rp, pll, samples);
        let (glow_img, glow_mem, glow_view, glow_sampler, glow_ds): (Option<vk::Image>, Option<vk::DeviceMemory>, Option<vk::ImageView>, Option<vk::Sampler>, Option<vk::DescriptorSet>) =
            if thermal {
                let (gimg, gmem, gview, gsampler) = Self::create_glow_texture(&device, &instance, pd);
                let gds = unsafe { device.allocate_descriptor_sets(&vk::DescriptorSetAllocateInfo::default().descriptor_pool(dpool).set_layouts(std::slice::from_ref(&dsl))) }.unwrap()[0];
                let bi = vk::DescriptorBufferInfo::default().buffer(ub).offset(0).range(std::mem::size_of::<Uniforms>() as u64);
                let w0 = vk::WriteDescriptorSet::default().dst_set(gds).dst_binding(0).descriptor_type(vk::DescriptorType::UNIFORM_BUFFER).buffer_info(std::slice::from_ref(&bi));
                let ci = vk::DescriptorImageInfo::default().image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL).image_view(gview).sampler(gsampler);
                let w1 = vk::WriteDescriptorSet::default().dst_set(gds).dst_binding(1).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&ci));
                let smi = vk::DescriptorImageInfo::default().image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL).image_view(sm_view).sampler(sm_sampler);
                let w2 = vk::WriteDescriptorSet::default().dst_set(gds).dst_binding(2).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&smi));
                unsafe { device.update_descriptor_sets(&[w0, w1, w2], &[]); }
                (Some(gimg), Some(gmem), Some(gview), Some(gsampler), Some(gds))
            } else { (None, None, None, None, None) };

        // Create a 1x1 white texture as default
        let (tex_img, tex_mem, tex_view, tex_sampler, _use_tex) = Self::create_default_texture(&device, &instance, pd);
        unsafe {
            let bi = vk::DescriptorBufferInfo::default().buffer(ub).offset(0).range(std::mem::size_of::<Uniforms>() as u64);
            let w = vk::WriteDescriptorSet::default().dst_set(dset).dst_binding(0).descriptor_type(vk::DescriptorType::UNIFORM_BUFFER).buffer_info(std::slice::from_ref(&bi));
            let ci = vk::DescriptorImageInfo::default().image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL).image_view(tex_view).sampler(tex_sampler);
            let w2 = vk::WriteDescriptorSet::default().dst_set(dset).dst_binding(1).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&ci));
            let smi = vk::DescriptorImageInfo::default().image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL).image_view(sm_view).sampler(sm_sampler);
            let w_sh = vk::WriteDescriptorSet::default().dst_set(dset).dst_binding(2).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&smi));
            device.update_descriptor_sets(&[w, w2, w_sh], &[]);
            let bi2 = vk::DescriptorBufferInfo::default().buffer(ub2).offset(0).range(std::mem::size_of::<Uniforms>() as u64);
            let w3 = vk::WriteDescriptorSet::default().dst_set(dset2).dst_binding(0).descriptor_type(vk::DescriptorType::UNIFORM_BUFFER).buffer_info(std::slice::from_ref(&bi2));
            // Overlay also needs a sampler binding (same white texture)
            let w4 = vk::WriteDescriptorSet::default().dst_set(dset2).dst_binding(1).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&ci));
            let w_sh2 = vk::WriteDescriptorSet::default().dst_set(dset2).dst_binding(2).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&smi));
            device.update_descriptor_sets(&[w3, w4, w_sh2], &[]);
            // Shadow pass: свой UBO (binding 0) + белая текстура в binding 1 и 2.
            // Карту теней в пассе теней НЕ сэмплируем (фрагментный шейдер сразу
            // выходит через shadow_params.w=1), а ссылаться на тот же образ, что
            // рендерим как depth-аттачмент, — ошибочно и рискованно.
            let bi_sh = vk::DescriptorBufferInfo::default().buffer(ub_shadow).offset(0).range(std::mem::size_of::<Uniforms>() as u64);
            let w_sh_buf = vk::WriteDescriptorSet::default().dst_set(dset_shadow).dst_binding(0).descriptor_type(vk::DescriptorType::UNIFORM_BUFFER).buffer_info(std::slice::from_ref(&bi_sh));
            let w_sh_tex = vk::WriteDescriptorSet::default().dst_set(dset_shadow).dst_binding(1).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&ci));
            let w_sh_sm = vk::WriteDescriptorSet::default().dst_set(dset_shadow).dst_binding(2).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&ci));
            device.update_descriptor_sets(&[w_sh_buf, w_sh_tex, w_sh_sm], &[]);
        }
        // ===== FXAA пост-пасс (только для режима fxaa) =====
        let mut present_rp = vk::RenderPass::null();
        let mut present_dsl = vk::DescriptorSetLayout::null();
        let mut present_layout = vk::PipelineLayout::null();
        let mut present_pipeline = vk::Pipeline::null();
        let mut present_set = vk::DescriptorSet::null();
        let mut present_sampler = vk::Sampler::null();
        let mut present_fbs: Vec<vk::Framebuffer> = Vec::new();
        if use_fxaa {
            present_dsl = unsafe { device.create_descriptor_set_layout(&vk::DescriptorSetLayoutCreateInfo::default().bindings(&[
                vk::DescriptorSetLayoutBinding::default().binding(0).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).descriptor_count(1).stage_flags(vk::ShaderStageFlags::FRAGMENT)
            ]), None) }.unwrap();
            present_sampler = unsafe { device.create_sampler(&vk::SamplerCreateInfo::default().mag_filter(vk::Filter::LINEAR).min_filter(vk::Filter::LINEAR).mipmap_mode(vk::SamplerMipmapMode::NEAREST).address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE).address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE).address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE).max_lod(1.0), None) }.unwrap();
            present_set = unsafe { device.allocate_descriptor_sets(&vk::DescriptorSetAllocateInfo::default().descriptor_pool(dpool).set_layouts(std::slice::from_ref(&present_dsl))) }.unwrap()[0];
            let sci = vk::DescriptorImageInfo::default().image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL).image_view(sc_col_view).sampler(present_sampler);
            unsafe { device.update_descriptor_sets(&[vk::WriteDescriptorSet::default().dst_set(present_set).dst_binding(0).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&sci))], &[]); }
            present_layout = unsafe { device.create_pipeline_layout(&vk::PipelineLayoutCreateInfo::default().set_layouts(std::slice::from_ref(&present_dsl)), None) }.unwrap();
            present_rp = Self::create_present_rp(&device, fmt);
            present_fbs = ivs.iter().map(|&view| unsafe { device.create_framebuffer(&vk::FramebufferCreateInfo::default().render_pass(present_rp).attachments(&[view]).width(ext.width).height(ext.height).layers(1), None) }.unwrap()).collect();
            present_pipeline = Self::create_fxaa_pipeline(&device, present_rp, present_layout);
        }

        let sem = vk::SemaphoreCreateInfo::default();
        let fi = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
        let ias = unsafe { device.create_semaphore(&sem, None) }.unwrap();
        let rfs = unsafe { device.create_semaphore(&sem, None) }.unwrap();
        let fen = unsafe { device.create_fence(&fi, None) }.unwrap();
        let (evb, evbm, evc) = Self::create_vb(&device, &instance, pd, &[0.0; 11]);
        let (eib, eibm, eic) = Self::create_ib(&device, &instance, pd, &[0]);
        let pverts: [f32; 6] = [-1.0, -1.0, 3.0, -1.0, -1.0, 3.0];
        let (pvb, pvbm, _) = Self::create_vb(&device, &instance, pd, &pverts);
        Self {
            entry, instance, surface, surface_loader: sfl, device, physical_device: pd,
            queue_family_index: qfi, queue, swapchain_loader: sl, swapchain: sc,
            swapchain_images: imgs, swapchain_image_views: ivs, swapchain_format: fmt,
            swapchain_extent: ext, render_pass: rp, pipeline, pipeline_layout: pll,
            descriptor_set_layout: dsl, descriptor_pool: dpool,
            descriptor_set_scene: dset, descriptor_set_ov: dset2, descriptor_set_shadow: dset_shadow,
            framebuffers: fbs, command_pool: cpool, command_buffers: cbs,
            image_available_semaphore: ias, render_finished_semaphore: rfs,
            fence: fen,
            vertex_buffer: vb, vertex_buffer_memory: vbm, index_buffer: ib,
            index_buffer_memory: ibm, edge_vertex_buffer: evb, edge_vertex_buffer_memory: evbm,
            edge_index_buffer: eib, edge_index_buffer_memory: eibm,
            uniform_buffer: ub, uniform_buffer_memory: ubm,
            uniform_buffer_ov: ub2, uniform_buffer_ov_memory: ubm2,
            shadow_uniform_buffer: ub_shadow, shadow_uniform_buffer_memory: ubm_shadow,
            vertex_count: vc, index_count: ic, edge_vertex_count: evc, edge_index_count: eic,
            current_buffer_size: 0, current_index_buffer_size: 0,
            current_edge_buffer_size: 0, current_edge_index_buffer_size: 0,
            line_width: 2.0,
            uniforms: ubo, uniforms_ov: ubo_ov,
            eye: Vec3::new(0.0, 0.0, 5.0), target: Vec3::zero(),
            depth_format: dfmt, depth_image: dimg, depth_image_memory: dimg_mem,
            depth_image_view: dimg_view,
            shadow_render_pass: smrp, shadow_map_size: sm_size,
            shadow_map: sm_map, shadow_map_memory: sm_mem,
            shadow_map_view: sm_view, shadow_map_sampler: sm_sampler,
            shadow_framebuffer: smfb,
            shadow_color: sm_col, shadow_color_memory: sm_col_mem,
            shadow_color_view: sm_col_view,
            shadow_bias: 0.0025,
            texture_image: tex_img, texture_image_memory: tex_mem,
            texture_image_view: tex_view, texture_sampler: tex_sampler,
            use_texture: _use_tex,
            textures: HashMap::new(),
            background: [0.63, 0.91, 1.0],
            aa, aa_samples: samples, use_fxaa,
            msaa_color: msaa_col, msaa_color_view: msaa_col_view, msaa_color_mem: msaa_col_mem,
            msaa_depth: msaa_dimg, msaa_depth_view: msaa_dimg_view, msaa_depth_mem: msaa_dimg_mem,
            scene_color: if use_fxaa { Some(sc_col) } else { None },
            scene_color_view: if use_fxaa { Some(sc_col_view) } else { None },
            scene_color_mem: if use_fxaa { Some(sc_col_mem) } else { None },
            present_render_pass: if use_fxaa { Some(present_rp) } else { None },
            present_framebuffers: present_fbs,
            present_pipeline: if use_fxaa && present_pipeline != vk::Pipeline::null() { Some(present_pipeline) } else { None },
            present_layout: if use_fxaa { Some(present_layout) } else { None },
            present_dsl: if use_fxaa { Some(present_dsl) } else { None },
            present_set: if use_fxaa { Some(present_set) } else { None },
            present_sampler: if use_fxaa { Some(present_sampler) } else { None },
            present_vb: pvb, present_vb_mem: pvbm,
            current_image_index: 0, window_width: ext.width, window_height: ext.height,
            wireframe: false,
            thermal_mode: thermal,
            additive_pipeline: additive_handle,
            ui_pipeline,
            glow_tex: if thermal {
                Some((glow_img.unwrap(), glow_mem.unwrap(), glow_view.unwrap(), glow_sampler.unwrap(), glow_ds.unwrap()))
            } else { None },
            glow_debug_done: false,
            heat_state: Vec::new(),
        }
    }

    fn create_default_texture(device: &Device, instance: &Instance, pd: vk::PhysicalDevice) -> (vk::Image, vk::DeviceMemory, vk::ImageView, vk::Sampler, bool) {
        let pixels: Vec<u8> = vec![255, 255, 255, 255];
        Self::create_texture_from_rgba(device, instance, pd, &pixels, 1, 1)
    }

    // Радиальная glow-текстура для тепловизора: белый центр -> чёрные края.
    // Аддитивный бленд (ONE,ONE) поверх тёмного фона даёт halo-свечение.
    fn create_glow_texture(device: &Device, instance: &Instance, pd: vk::PhysicalDevice) -> (vk::Image, vk::DeviceMemory, vk::ImageView, vk::Sampler) {
        const S: u32 = 64;
        let mut px = Vec::with_capacity((S * S * 4) as usize);
        for y in 0..S {
            for x in 0..S {
                let nx = x as f32 / (S - 1) as f32 * 2.0 - 1.0;
                let ny = y as f32 / (S - 1) as f32 * 2.0 - 1.0;
                let r = (nx * nx + ny * ny).sqrt();
                let v = (1.0 - r).max(0.0);
                let v = v * v * (3.0 - 2.0 * v);
                let c = (v * 255.0) as u8;
                px.extend_from_slice(&[c, c, c, 255]);
            }
        }
        let (img, mem, view, sampler, _) = Self::create_texture_from_rgba(device, instance, pd, &px, S, S);
        (img, mem, view, sampler)
    }

    // Аддитивный пайплайн (blend ONE,ONE) для глоу: reuse сценовых шейдеров,
    // глубина тестируется, но НЕ пишется (глоу не перекрывает геометрию).
    fn create_additive_pipeline(device: &Device, render_pass: vk::RenderPass, layout: vk::PipelineLayout, samples: u32) -> vk::Pipeline {
        use ash::vk;
        let name = CString::new("main").unwrap();
        let vc = crate::render::shader::ShaderManager::load_shader("shaders/vert.spv");
        let fc = crate::render::shader::ShaderManager::load_shader("shaders/frag.spv");
        let vm = unsafe { device.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&vc), None) }.unwrap();
        let fm = unsafe { device.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&fc), None) }.unwrap();
        let vs = vk::PipelineShaderStageCreateInfo::default().stage(vk::ShaderStageFlags::VERTEX).module(vm).name(&name);
        let fs = vk::PipelineShaderStageCreateInfo::default().stage(vk::ShaderStageFlags::FRAGMENT).module(fm).name(&name);
        let stages = [vs, fs];
        let binding_desc = vk::VertexInputBindingDescription::default().binding(0).stride(11 * 4).input_rate(vk::VertexInputRate::VERTEX);
        let attr_descs = [
            vk::VertexInputAttributeDescription::default().location(0).binding(0).format(vk::Format::R32G32B32_SFLOAT).offset(0),
            vk::VertexInputAttributeDescription::default().location(1).binding(0).format(vk::Format::R32G32B32_SFLOAT).offset(12),
            vk::VertexInputAttributeDescription::default().location(2).binding(0).format(vk::Format::R32G32_SFLOAT).offset(36),
            vk::VertexInputAttributeDescription::default().location(3).binding(0).format(vk::Format::R32G32B32_SFLOAT).offset(24),
        ];
        let vertex_input = vk::PipelineVertexInputStateCreateInfo::default().vertex_binding_descriptions(std::slice::from_ref(&binding_desc)).vertex_attribute_descriptions(&attr_descs);
        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default().topology(vk::PrimitiveTopology::TRIANGLE_LIST);
        let viewport = vk::Viewport::default().x(0.0).y(0.0).width(1.0).height(1.0).min_depth(0.0).max_depth(1.0);
        let scissor = vk::Rect2D::default().offset(vk::Offset2D { x: 0, y: 0 }).extent(vk::Extent2D { width: 1, height: 1 });
        let viewport_state = vk::PipelineViewportStateCreateInfo::default().viewports(std::slice::from_ref(&viewport)).scissors(std::slice::from_ref(&scissor));
        let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let dynamic_state = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
        let rasterizer = vk::PipelineRasterizationStateCreateInfo::default().polygon_mode(vk::PolygonMode::FILL).line_width(1.0).cull_mode(vk::CullModeFlags::NONE).front_face(vk::FrontFace::COUNTER_CLOCKWISE);
        let multisample = vk::PipelineMultisampleStateCreateInfo::default().rasterization_samples(crate::render::pipeline::samples_flags(samples));
        let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default().depth_test_enable(false).depth_write_enable(false);
        let cba = vk::PipelineColorBlendAttachmentState::default()
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::ONE)
            .dst_color_blend_factor(vk::BlendFactor::ONE)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE)
            .alpha_blend_op(vk::BlendOp::ADD)
            .color_write_mask(vk::ColorComponentFlags::RGBA);
        let color_blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(std::slice::from_ref(&cba));
        let pinfo = vk::GraphicsPipelineCreateInfo::default().stages(&stages).vertex_input_state(&vertex_input).input_assembly_state(&input_assembly).viewport_state(&viewport_state).dynamic_state(&dynamic_state).rasterization_state(&rasterizer).multisample_state(&multisample).depth_stencil_state(&depth_stencil).color_blend_state(&color_blend).layout(layout).render_pass(render_pass).subpass(0);
        let result = unsafe { device.create_graphics_pipelines(vk::PipelineCache::null(), std::slice::from_ref(&pinfo), None) };
        unsafe { device.destroy_shader_module(vm, None); device.destroy_shader_module(fm, None); }
        match result {
            Ok(p) => p[0],
            Err(e) => panic!("Failed to create additive pipeline: {:?}", e),
        }
    }

    fn create_texture_from_rgba(device: &Device, instance: &Instance, pd: vk::PhysicalDevice, pixels: &[u8], w: u32, h: u32) -> (vk::Image, vk::DeviceMemory, vk::ImageView, vk::Sampler, bool) {
        let img_size = (w * h * 4) as u64;
        let buffer_info = vk::BufferCreateInfo::default().size(img_size).usage(vk::BufferUsageFlags::TRANSFER_SRC).sharing_mode(vk::SharingMode::EXCLUSIVE);
        let staging_buf = unsafe { device.create_buffer(&buffer_info, None) }.unwrap();
        let mr = unsafe { device.get_buffer_memory_requirements(staging_buf) };
        let mt = Self::mt(instance, pd, mr.memory_type_bits, vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT);
        let staging_mem = unsafe { device.allocate_memory(&vk::MemoryAllocateInfo::default().allocation_size(mr.size).memory_type_index(mt), None) }.unwrap();
        unsafe { device.bind_buffer_memory(staging_buf, staging_mem, 0) }.unwrap();
        unsafe { let p = device.map_memory(staging_mem, 0, img_size, vk::MemoryMapFlags::empty()).unwrap(); std::ptr::copy_nonoverlapping(pixels.as_ptr(), p as *mut u8, pixels.len()); device.unmap_memory(staging_mem); }

        let image_info = vk::ImageCreateInfo::default().image_type(vk::ImageType::TYPE_2D).format(vk::Format::R8G8B8A8_SRGB).extent(vk::Extent3D{width:w,height:h,depth:1}).mip_levels(1).array_layers(1).samples(vk::SampleCountFlags::TYPE_1).tiling(vk::ImageTiling::OPTIMAL).usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED).sharing_mode(vk::SharingMode::EXCLUSIVE);
        let img = unsafe { device.create_image(&image_info, None) }.unwrap();
        let mr2 = unsafe { device.get_image_memory_requirements(img) };
        let mt2 = Self::mt(instance, pd, mr2.memory_type_bits, vk::MemoryPropertyFlags::DEVICE_LOCAL);
        let img_mem = unsafe { device.allocate_memory(&vk::MemoryAllocateInfo::default().allocation_size(mr2.size).memory_type_index(mt2), None) }.unwrap();
        unsafe { device.bind_image_memory(img, img_mem, 0) }.unwrap();

        // Transition + copy
        let cpool = unsafe { device.create_command_pool(&vk::CommandPoolCreateInfo::default().queue_family_index(0), None) }.unwrap();
        let cbuf = unsafe { device.allocate_command_buffers(&vk::CommandBufferAllocateInfo::default().command_pool(cpool).level(vk::CommandBufferLevel::PRIMARY).command_buffer_count(1)) }.unwrap()[0];
        unsafe {
            device.begin_command_buffer(cbuf, &vk::CommandBufferBeginInfo::default()).unwrap();
            // Undefined -> TransferDst
            let barrier = vk::ImageMemoryBarrier::default().image(img).subresource_range(vk::ImageSubresourceRange::default().aspect_mask(vk::ImageAspectFlags::COLOR).base_mip_level(0).level_count(1).base_array_layer(0).layer_count(1)).old_layout(vk::ImageLayout::UNDEFINED).new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL).src_access_mask(vk::AccessFlags::empty()).dst_access_mask(vk::AccessFlags::TRANSFER_WRITE);
            device.cmd_pipeline_barrier(cbuf, vk::PipelineStageFlags::TOP_OF_PIPE, vk::PipelineStageFlags::TRANSFER, vk::DependencyFlags::empty(), &[], &[], &[barrier]);
            device.cmd_copy_buffer_to_image(cbuf, staging_buf, img, vk::ImageLayout::TRANSFER_DST_OPTIMAL, &[vk::BufferImageCopy::default().buffer_offset(0).buffer_row_length(0).buffer_image_height(0).image_subresource(vk::ImageSubresourceLayers::default().aspect_mask(vk::ImageAspectFlags::COLOR).layer_count(1)).image_offset(vk::Offset3D{x:0,y:0,z:0}).image_extent(vk::Extent3D{width:w,height:h,depth:1})]);
            // TransferDst -> ShaderReadOnly
            let barrier2 = vk::ImageMemoryBarrier::default().image(img).subresource_range(vk::ImageSubresourceRange::default().aspect_mask(vk::ImageAspectFlags::COLOR).base_mip_level(0).level_count(1).base_array_layer(0).layer_count(1)).old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL).new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL).src_access_mask(vk::AccessFlags::TRANSFER_WRITE).dst_access_mask(vk::AccessFlags::SHADER_READ);
            device.cmd_pipeline_barrier(cbuf, vk::PipelineStageFlags::TRANSFER, vk::PipelineStageFlags::FRAGMENT_SHADER, vk::DependencyFlags::empty(), &[], &[], &[barrier2]);
            device.end_command_buffer(cbuf).unwrap();
        }
        let q = unsafe { device.get_device_queue(0, 0) };
        let si = vk::SubmitInfo::default().command_buffers(std::slice::from_ref(&cbuf));
        unsafe { device.queue_submit(q, std::slice::from_ref(&si), vk::Fence::null()) }.unwrap();
        unsafe { device.queue_wait_idle(q) }.unwrap();
        unsafe { device.destroy_command_pool(cpool, None); }
        unsafe { device.destroy_buffer(staging_buf, None); }
        unsafe { device.free_memory(staging_mem, None); }

        let view = unsafe { device.create_image_view(&vk::ImageViewCreateInfo::default().image(img).view_type(vk::ImageViewType::TYPE_2D).format(vk::Format::R8G8B8A8_SRGB).subresource_range(vk::ImageSubresourceRange::default().aspect_mask(vk::ImageAspectFlags::COLOR).base_mip_level(0).level_count(1).base_array_layer(0).layer_count(1)), None) }.unwrap();
        let sampler = unsafe { device.create_sampler(&vk::SamplerCreateInfo::default().mag_filter(vk::Filter::LINEAR).min_filter(vk::Filter::LINEAR).mipmap_mode(vk::SamplerMipmapMode::LINEAR).address_mode_u(vk::SamplerAddressMode::REPEAT).address_mode_v(vk::SamplerAddressMode::REPEAT).address_mode_w(vk::SamplerAddressMode::REPEAT).max_lod(1.0), None) }.unwrap();
        (img, img_mem, view, sampler, true)
    }

    // Возвращает descriptor set + aspect для данной текстуры (кэширует)
    pub fn get_or_create_texture(&mut self, path: &str) -> (vk::DescriptorSet, f32) {
        if let Some((entry, ds)) = self.textures.get(path) {
            return (*ds, entry.width as f32 / entry.height as f32);
        }
        println!("[Texture] Loading per-object: {}", path);
        let img_data = match image::open(path) {
            Ok(img) => img.to_rgba8(),
            Err(e) => { println!("[Texture] Failed: {}", e); return (self.descriptor_set_scene, 1.0); }
        };
        let (img_w, img_h) = img_data.dimensions();
        let pixels = img_data.into_raw();
        let (img, mem, view, sampler, _) = Self::create_texture_from_rgba(&self.device, &self.instance, self.physical_device, &pixels, img_w, img_h);
        let ds = unsafe { self.device.allocate_descriptor_sets(&vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(self.descriptor_pool).set_layouts(std::slice::from_ref(&self.descriptor_set_layout))) }.unwrap()[0];
        // Привязываем UBO (binding 0 = сценовый uniform_buffer) + текстуру (binding 1)
        let bi = vk::DescriptorBufferInfo::default().buffer(self.uniform_buffer).offset(0).range(std::mem::size_of::<Uniforms>() as u64);
        let ci = vk::DescriptorImageInfo::default().image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL).image_view(view).sampler(sampler);
        let smi = vk::DescriptorImageInfo::default().image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL).image_view(self.shadow_map_view).sampler(self.shadow_map_sampler);
        let writes = [
            vk::WriteDescriptorSet::default().dst_set(ds).dst_binding(0).descriptor_type(vk::DescriptorType::UNIFORM_BUFFER).buffer_info(std::slice::from_ref(&bi)),
            vk::WriteDescriptorSet::default().dst_set(ds).dst_binding(1).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&ci)),
            vk::WriteDescriptorSet::default().dst_set(ds).dst_binding(2).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&smi)),
        ];
        unsafe { self.device.update_descriptor_sets(&writes, &[]); }
        let entry = TexEntry { image: img, memory: mem, view, sampler, width: img_w, height: img_h };
        let aspect = img_w as f32 / img_h as f32;
        self.textures.insert(path.to_string(), (entry, ds));
        (ds, aspect)
    }

    fn find_qf(instance: &Instance, pd: vk::PhysicalDevice, surface: vk::SurfaceKHR, sfl: &surface::Instance) -> Option<u32> {
        let qfs = unsafe { instance.get_physical_device_queue_family_properties(pd) };
        for (i, f) in qfs.iter().enumerate() {
            if f.queue_flags.contains(vk::QueueFlags::GRAPHICS) && unsafe { sfl.get_physical_device_surface_support(pd, i as u32, surface) }.unwrap_or(false) { return Some(i as u32); }
        }
        None
    }

    fn create_swapchain(
        pd: vk::PhysicalDevice, sl: &swapchain::Device,
        surface: vk::SurfaceKHR, sfl: &surface::Instance, width: u32, height: u32,
    ) -> Option<(vk::SwapchainKHR, vk::Format, vk::Extent2D, Vec<vk::Image>)> {
        let caps = unsafe { sfl.get_physical_device_surface_capabilities(pd, surface) }.ok()?;
        if caps.current_extent.width == 0 || caps.current_extent.height == 0 { return None; }
        let formats = unsafe { sfl.get_physical_device_surface_formats(pd, surface) }.unwrap();
        let fmt = formats.iter().find(|f| {
            f.format == vk::Format::B8G8R8A8_SRGB || f.format == vk::Format::R8G8B8A8_SRGB
        }).unwrap_or(&formats[0]);
        let extent = match caps.current_extent {
            vk::Extent2D { width: u32::MAX, .. } => vk::Extent2D { width, height },
            e => e,
        };
        let mut ic = caps.min_image_count + 1;
        if caps.max_image_count > 0 && ic > caps.max_image_count { ic = caps.max_image_count; }
        let sci = vk::SwapchainCreateInfoKHR::default()
            .surface(surface).min_image_count(ic)
            .image_format(fmt.format).image_color_space(fmt.color_space)
            .image_extent(extent).image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(caps.current_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(vk::PresentModeKHR::FIFO).clipped(true);
        let sc = unsafe { sl.create_swapchain(&sci, None) }.unwrap();
        let imgs = unsafe { sl.get_swapchain_images(sc) }.unwrap();
        Some((sc, fmt.format, extent, imgs))
    }

    fn find_fmt(_device: &Device, instance: &Instance, pd: vk::PhysicalDevice, candidates: &[vk::Format], tiling: vk::ImageTiling, features: vk::FormatFeatureFlags) -> Option<vk::Format> {
        for &f in candidates { let p = unsafe { instance.get_physical_device_format_properties(pd, f) }; let ok = match tiling { vk::ImageTiling::LINEAR => p.linear_tiling_features, _ => p.optimal_tiling_features }; if ok.contains(features) { return Some(f); } } None
    }

    // Карта теней: простая depth-текстура, пригодная и как аттачмент рендер-пасса,
    // и как сэмплер в основном pass (SAMPLED). D32_SFLOAT обязателен в Vulkan 1.0.
    fn create_shadow_map(device: &Device, instance: &Instance, pd: vk::PhysicalDevice, size: u32) -> (vk::Image, vk::DeviceMemory, vk::ImageView, vk::Sampler) {
        let df = vk::Format::D32_SFLOAT;
        let ii = vk::ImageCreateInfo::default().image_type(vk::ImageType::TYPE_2D).format(df).extent(vk::Extent3D { width: size, height: size, depth: 1 }).mip_levels(1).array_layers(1).samples(vk::SampleCountFlags::TYPE_1).tiling(vk::ImageTiling::OPTIMAL).usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED).sharing_mode(vk::SharingMode::EXCLUSIVE);
        let img = unsafe { device.create_image(&ii, None) }.unwrap();
        let mr = unsafe { device.get_image_memory_requirements(img) };
        let mt = Self::mt(instance, pd, mr.memory_type_bits, vk::MemoryPropertyFlags::DEVICE_LOCAL);
        let mem = unsafe { device.allocate_memory(&vk::MemoryAllocateInfo::default().allocation_size(mr.size).memory_type_index(mt), None) }.unwrap();
        unsafe { device.bind_image_memory(img, mem, 0) }.unwrap();
        let view = unsafe { device.create_image_view(&vk::ImageViewCreateInfo::default().image(img).view_type(vk::ImageViewType::TYPE_2D).format(df).subresource_range(vk::ImageSubresourceRange::default().aspect_mask(vk::ImageAspectFlags::DEPTH).base_mip_level(0).level_count(1).base_array_layer(0).layer_count(1)), None) }.unwrap();
        let sampler = unsafe { device.create_sampler(&vk::SamplerCreateInfo::default().mag_filter(vk::Filter::LINEAR).min_filter(vk::Filter::LINEAR).mipmap_mode(vk::SamplerMipmapMode::NEAREST).address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE).address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE).address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE).max_lod(1.0), None) }.unwrap();
        (img, mem, view, sampler)
    }

    // Фиктивный цветовой таргет (для пасса теней), чтобы подпасс имел цветовой
    // аттачмент — так пайплайн создаётся на всех драйверах, в т.ч. программных.
    fn create_color_target(device: &Device, instance: &Instance, pd: vk::PhysicalDevice, size: u32) -> (vk::Image, vk::DeviceMemory, vk::ImageView) {
        let fmt = vk::Format::R8G8B8A8_UNORM;
        let ii = vk::ImageCreateInfo::default().image_type(vk::ImageType::TYPE_2D).format(fmt).extent(vk::Extent3D { width: size, height: size, depth: 1 }).mip_levels(1).array_layers(1).samples(vk::SampleCountFlags::TYPE_1).tiling(vk::ImageTiling::OPTIMAL).usage(vk::ImageUsageFlags::COLOR_ATTACHMENT).sharing_mode(vk::SharingMode::EXCLUSIVE);
        let img = unsafe { device.create_image(&ii, None) }.unwrap();
        let mr = unsafe { device.get_image_memory_requirements(img) };
        let mt = Self::mt(instance, pd, mr.memory_type_bits, vk::MemoryPropertyFlags::DEVICE_LOCAL);
        let mem = unsafe { device.allocate_memory(&vk::MemoryAllocateInfo::default().allocation_size(mr.size).memory_type_index(mt), None) }.unwrap();
        unsafe { device.bind_image_memory(img, mem, 0) }.unwrap();
        let view = unsafe { device.create_image_view(&vk::ImageViewCreateInfo::default().image(img).view_type(vk::ImageViewType::TYPE_2D).format(fmt).subresource_range(vk::ImageSubresourceRange::default().aspect_mask(vk::ImageAspectFlags::COLOR).base_mip_level(0).level_count(1).base_array_layer(0).layer_count(1)), None) }.unwrap();
        (img, mem, view)
    }

    // Рендер-пасс теней: фиктивный цветовой аттачмент (индекс 0) + глубина
    // (индекс 1). Цвет не читаем, глубина пишется в карту теней.
    fn create_shadow_rp(device: &Device, color_fmt: vk::Format, dfmt: vk::Format) -> vk::RenderPass {
        let ca = vk::AttachmentDescription::default().format(color_fmt).samples(vk::SampleCountFlags::TYPE_1).load_op(vk::AttachmentLoadOp::CLEAR).store_op(vk::AttachmentStoreOp::DONT_CARE).stencil_load_op(vk::AttachmentLoadOp::DONT_CARE).stencil_store_op(vk::AttachmentStoreOp::DONT_CARE).initial_layout(vk::ImageLayout::UNDEFINED).final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
        let da = vk::AttachmentDescription::default().format(dfmt).samples(vk::SampleCountFlags::TYPE_1).load_op(vk::AttachmentLoadOp::CLEAR).store_op(vk::AttachmentStoreOp::STORE).stencil_load_op(vk::AttachmentLoadOp::DONT_CARE).stencil_store_op(vk::AttachmentStoreOp::DONT_CARE).initial_layout(vk::ImageLayout::UNDEFINED).final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);
        let cr = vk::AttachmentReference::default().attachment(0).layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
        let dr = vk::AttachmentReference::default().attachment(1).layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);
        let sp = vk::SubpassDescription::default().pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS).color_attachments(std::slice::from_ref(&cr)).depth_stencil_attachment(&dr);
        let dep = vk::SubpassDependency::default().src_subpass(vk::SUBPASS_EXTERNAL).dst_subpass(0).src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS).dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS).dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE);
        let atts = [ca, da];
        unsafe { device.create_render_pass(&vk::RenderPassCreateInfo::default().attachments(&atts).subpasses(std::slice::from_ref(&sp)).dependencies(std::slice::from_ref(&dep)), None) }.unwrap()
    }

    fn create_depth(device: &Device, instance: &Instance, pd: vk::PhysicalDevice, w: u32, h: u32, samples: u32) -> (vk::Image, vk::DeviceMemory, vk::ImageView, vk::Format) {
        let df = Self::find_fmt(device, instance, pd, &[vk::Format::D32_SFLOAT, vk::Format::D24_UNORM_S8_UINT], vk::ImageTiling::OPTIMAL, vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT).unwrap();
        let ii = vk::ImageCreateInfo::default().image_type(vk::ImageType::TYPE_2D).format(df).extent(vk::Extent3D { width: w, height: h, depth: 1 }).mip_levels(1).array_layers(1).samples(samples_flags(samples)).tiling(vk::ImageTiling::OPTIMAL).usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT).sharing_mode(vk::SharingMode::EXCLUSIVE);
        let img = unsafe { device.create_image(&ii, None) }.unwrap();
        let mr = unsafe { device.get_image_memory_requirements(img) };
        let mt = Self::mt(instance, pd, mr.memory_type_bits, vk::MemoryPropertyFlags::DEVICE_LOCAL);
        let ai = vk::MemoryAllocateInfo::default().allocation_size(mr.size).memory_type_index(mt);
        let mem = unsafe { device.allocate_memory(&ai, None) }.unwrap();
        unsafe { device.bind_image_memory(img, mem, 0) }.unwrap();
        let vi = vk::ImageViewCreateInfo::default()
            .image(img).view_type(vk::ImageViewType::TYPE_2D).format(df)
            .subresource_range(vk::ImageSubresourceRange::default()
                .aspect_mask(vk::ImageAspectFlags::DEPTH)
                .base_mip_level(0).level_count(1)
                .base_array_layer(0).layer_count(1));
        let view = unsafe { device.create_image_view(&vi, None) }.unwrap();
        (img, mem, view, df)
    }

    // MSAA цветовой таргет (источник resolve). single-таргет = сцена во FXAA.
    fn create_color_samples(device: &Device, instance: &Instance, pd: vk::PhysicalDevice, fmt: vk::Format, w: u32, h: u32, samples: u32, sampled: bool) -> (vk::Image, vk::DeviceMemory, vk::ImageView) {
        let usage = if samples > 1 {
            vk::ImageUsageFlags::COLOR_ATTACHMENT
        } else {
            let sampled_f = if sampled { vk::ImageUsageFlags::SAMPLED } else { vk::ImageUsageFlags::empty() };
            vk::ImageUsageFlags::COLOR_ATTACHMENT | sampled_f
        };
        let ii = vk::ImageCreateInfo::default().image_type(vk::ImageType::TYPE_2D).format(fmt).extent(vk::Extent3D { width: w, height: h, depth: 1 }).mip_levels(1).array_layers(1).samples(samples_flags(samples)).tiling(vk::ImageTiling::OPTIMAL).usage(usage).sharing_mode(vk::SharingMode::EXCLUSIVE);
        let img = unsafe { device.create_image(&ii, None) }.unwrap();
        let mr = unsafe { device.get_image_memory_requirements(img) };
        let mt = Self::mt(instance, pd, mr.memory_type_bits, vk::MemoryPropertyFlags::DEVICE_LOCAL);
        let mem = unsafe { device.allocate_memory(&vk::MemoryAllocateInfo::default().allocation_size(mr.size).memory_type_index(mt), None) }.unwrap();
        unsafe { device.bind_image_memory(img, mem, 0) }.unwrap();
        let view = unsafe { device.create_image_view(&vk::ImageViewCreateInfo::default().image(img).view_type(vk::ImageViewType::TYPE_2D).format(fmt).subresource_range(vk::ImageSubresourceRange::default().aspect_mask(vk::ImageAspectFlags::COLOR).base_mip_level(0).level_count(1).base_array_layer(0).layer_count(1)), None) }.unwrap();
        (img, mem, view)
    }

    fn create_rp(device: &Device, fmt: vk::Format, dfmt: vk::Format) -> vk::RenderPass {
        let ca = vk::AttachmentDescription::default().format(fmt).samples(vk::SampleCountFlags::TYPE_1).load_op(vk::AttachmentLoadOp::CLEAR).store_op(vk::AttachmentStoreOp::STORE).stencil_load_op(vk::AttachmentLoadOp::DONT_CARE).stencil_store_op(vk::AttachmentStoreOp::DONT_CARE).initial_layout(vk::ImageLayout::UNDEFINED).final_layout(vk::ImageLayout::PRESENT_SRC_KHR);
        let da = vk::AttachmentDescription::default().format(dfmt).samples(vk::SampleCountFlags::TYPE_1).load_op(vk::AttachmentLoadOp::CLEAR).store_op(vk::AttachmentStoreOp::DONT_CARE).stencil_load_op(vk::AttachmentLoadOp::DONT_CARE).stencil_store_op(vk::AttachmentStoreOp::DONT_CARE).initial_layout(vk::ImageLayout::UNDEFINED).final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);
        let cr = vk::AttachmentReference::default().attachment(0).layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
        let dr = vk::AttachmentReference::default().attachment(1).layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);
        let sp = vk::SubpassDescription::default()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(std::slice::from_ref(&cr))
            .depth_stencil_attachment(&dr);
        let dep = vk::SubpassDependency::default()
            .src_subpass(vk::SUBPASS_EXTERNAL).dst_subpass(0)
            .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS)
            .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS)
            .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE);
        let atts = [ca, da];
        unsafe {
            device.create_render_pass(
                &vk::RenderPassCreateInfo::default().attachments(&atts).subpasses(std::slice::from_ref(&sp)).dependencies(std::slice::from_ref(&dep)),
                None,
            )
        }.unwrap()
    }

    // Рендер-пасс сцены во FXAA: цветовой таргет = offscreen (финал SHADER_READ_ONLY).
    fn create_rp_fxaa(device: &Device, fmt: vk::Format, dfmt: vk::Format) -> vk::RenderPass {
        let ca = vk::AttachmentDescription::default().format(fmt).samples(vk::SampleCountFlags::TYPE_1).load_op(vk::AttachmentLoadOp::CLEAR).store_op(vk::AttachmentStoreOp::STORE).stencil_load_op(vk::AttachmentLoadOp::DONT_CARE).stencil_store_op(vk::AttachmentStoreOp::DONT_CARE).initial_layout(vk::ImageLayout::UNDEFINED).final_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
        let da = vk::AttachmentDescription::default().format(dfmt).samples(vk::SampleCountFlags::TYPE_1).load_op(vk::AttachmentLoadOp::CLEAR).store_op(vk::AttachmentStoreOp::DONT_CARE).stencil_load_op(vk::AttachmentLoadOp::DONT_CARE).stencil_store_op(vk::AttachmentStoreOp::DONT_CARE).initial_layout(vk::ImageLayout::UNDEFINED).final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);
        let cr = vk::AttachmentReference::default().attachment(0).layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
        let dr = vk::AttachmentReference::default().attachment(1).layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);
        let sp = vk::SubpassDescription::default().pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS).color_attachments(std::slice::from_ref(&cr)).depth_stencil_attachment(&dr);
        let dep = vk::SubpassDependency::default().src_subpass(vk::SUBPASS_EXTERNAL).dst_subpass(0).src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS).dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS).dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE);
        let atts = [ca, da];
        unsafe { device.create_render_pass(&vk::RenderPassCreateInfo::default().attachments(&atts).subpasses(std::slice::from_ref(&sp)).dependencies(std::slice::from_ref(&dep)), None) }.unwrap()
    }

    // MSAA рендер-пасс: мультисэмпловый цвет + resolve в swapchain + MSAA-глубина.
    fn create_msaa_rp(device: &Device, fmt: vk::Format, dfmt: vk::Format, samples: u32) -> vk::RenderPass {
        let sf = samples_flags(samples);
        let nearby = vk::AttachmentDescription::default().format(fmt).samples(sf).load_op(vk::AttachmentLoadOp::CLEAR).store_op(vk::AttachmentStoreOp::DONT_CARE).stencil_load_op(vk::AttachmentLoadOp::DONT_CARE).stencil_store_op(vk::AttachmentStoreOp::DONT_CARE).initial_layout(vk::ImageLayout::UNDEFINED).final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
        let res = vk::AttachmentDescription::default().format(fmt).samples(vk::SampleCountFlags::TYPE_1).load_op(vk::AttachmentLoadOp::DONT_CARE).store_op(vk::AttachmentStoreOp::STORE).stencil_load_op(vk::AttachmentLoadOp::DONT_CARE).stencil_store_op(vk::AttachmentStoreOp::DONT_CARE).initial_layout(vk::ImageLayout::UNDEFINED).final_layout(vk::ImageLayout::PRESENT_SRC_KHR);
        let da = vk::AttachmentDescription::default().format(dfmt).samples(sf).load_op(vk::AttachmentLoadOp::CLEAR).store_op(vk::AttachmentStoreOp::DONT_CARE).stencil_load_op(vk::AttachmentLoadOp::DONT_CARE).stencil_store_op(vk::AttachmentStoreOp::DONT_CARE).initial_layout(vk::ImageLayout::UNDEFINED).final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);
        let cr = vk::AttachmentReference::default().attachment(0).layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
        let rref = vk::AttachmentReference::default().attachment(1).layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
        let dr = vk::AttachmentReference::default().attachment(2).layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);
        let sp = vk::SubpassDescription::default().pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS).color_attachments(std::slice::from_ref(&cr)).resolve_attachments(std::slice::from_ref(&rref)).depth_stencil_attachment(&dr);
        let dep = vk::SubpassDependency::default().src_subpass(vk::SUBPASS_EXTERNAL).dst_subpass(0).src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS).dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS).dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE);
        let atts = [nearby, res, da];
        unsafe { device.create_render_pass(&vk::RenderPassCreateInfo::default().attachments(&atts).subpasses(std::slice::from_ref(&sp)).dependencies(std::slice::from_ref(&dep)), None) }.unwrap()
    }

    // Простой рендер-пасс для полноэкранного пост-пасса (FXAA) → swapchain.
    fn create_present_rp(device: &Device, fmt: vk::Format) -> vk::RenderPass {
        let ca = vk::AttachmentDescription::default().format(fmt).samples(vk::SampleCountFlags::TYPE_1).load_op(vk::AttachmentLoadOp::DONT_CARE).store_op(vk::AttachmentStoreOp::STORE).stencil_load_op(vk::AttachmentLoadOp::DONT_CARE).stencil_store_op(vk::AttachmentStoreOp::DONT_CARE).initial_layout(vk::ImageLayout::UNDEFINED).final_layout(vk::ImageLayout::PRESENT_SRC_KHR);
        let cr = vk::AttachmentReference::default().attachment(0).layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
        let sp = vk::SubpassDescription::default().pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS).color_attachments(std::slice::from_ref(&cr));
        let dep = vk::SubpassDependency::default().src_subpass(vk::SUBPASS_EXTERNAL).dst_subpass(0).src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT).dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT).dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE);
        let atts = [ca];
        unsafe { device.create_render_pass(&vk::RenderPassCreateInfo::default().attachments(&atts).subpasses(std::slice::from_ref(&sp)).dependencies(std::slice::from_ref(&dep)), None) }.unwrap()
    }

    // Полноэкранный FXAA-пайплайн: 3 вершины без буферов, семплирует scene_color.
    fn create_fxaa_pipeline(device: &Device, render_pass: vk::RenderPass, layout: vk::PipelineLayout) -> vk::Pipeline {
        fn read_spv(p: &str) -> Vec<u32> {
            let b = std::fs::read(p).expect(&format!("Failed to read shader {}", p));
            b.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect()
        }
        let name = CString::new("main").unwrap();
        let vm = unsafe { device.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&read_spv("shaders/fullscreen_vert.spv")), None) }.unwrap();
        let fm = unsafe { device.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&read_spv("shaders/fxaa_frag.spv")), None) }.unwrap();
        let vs = vk::PipelineShaderStageCreateInfo::default().stage(vk::ShaderStageFlags::VERTEX).module(vm).name(&name);
        let fs = vk::PipelineShaderStageCreateInfo::default().stage(vk::ShaderStageFlags::FRAGMENT).module(fm).name(&name);
        let stages = [vs, fs];
        let ia = vk::PipelineInputAssemblyStateCreateInfo::default().topology(vk::PrimitiveTopology::TRIANGLE_LIST);
        // Вершинный вход: только inPos (2 floats).
        let pbinding = vk::VertexInputBindingDescription::default().binding(0).stride(2 * 4).input_rate(vk::VertexInputRate::VERTEX);
        let pattr = [vk::VertexInputAttributeDescription::default().location(0).binding(0).format(vk::Format::R32G32_SFLOAT).offset(0)];
        let vis = vk::PipelineVertexInputStateCreateInfo::default().vertex_binding_descriptions(std::slice::from_ref(&pbinding)).vertex_attribute_descriptions(&pattr);
        let vp = vk::Viewport::default().x(0.0).y(0.0).width(1.0).height(1.0).min_depth(0.0).max_depth(1.0);
        let sc = vk::Rect2D::default().offset(vk::Offset2D { x: 0, y: 0 }).extent(vk::Extent2D { width: 1, height: 1 });
        let vps = vk::PipelineViewportStateCreateInfo::default().viewports(std::slice::from_ref(&vp)).scissors(std::slice::from_ref(&sc));
        let dss = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let dyn_state = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dss);
        let ras = vk::PipelineRasterizationStateCreateInfo::default().polygon_mode(vk::PolygonMode::FILL).line_width(1.0).cull_mode(vk::CullModeFlags::NONE).front_face(vk::FrontFace::COUNTER_CLOCKWISE);
        let ms = vk::PipelineMultisampleStateCreateInfo::default().rasterization_samples(vk::SampleCountFlags::TYPE_1);
        let cba = vk::PipelineColorBlendAttachmentState::default().color_write_mask(vk::ColorComponentFlags::RGBA).blend_enable(false);
        let cb = vk::PipelineColorBlendStateCreateInfo::default().attachments(std::slice::from_ref(&cba));
        let pinfo = vk::GraphicsPipelineCreateInfo::default().stages(&stages).vertex_input_state(&vis).input_assembly_state(&ia).viewport_state(&vps).dynamic_state(&dyn_state).rasterization_state(&ras).multisample_state(&ms).color_blend_state(&cb).layout(layout).render_pass(render_pass).subpass(0);
        let pl = unsafe { device.create_graphics_pipelines(vk::PipelineCache::null(), std::slice::from_ref(&pinfo), None) }.unwrap();
        unsafe { device.destroy_shader_module(vm, None); device.destroy_shader_module(fm, None); }
        if pl[0] != vk::Pipeline::null() {
            println!("[Vulkan] FXAA pipeline created");
        }
        pl[0]
    }

    fn create_desc(device: &Device) -> (vk::DescriptorSetLayout, vk::DescriptorPool, vk::DescriptorSet, vk::DescriptorSet, vk::DescriptorSet) {
        let bindings = [
            vk::DescriptorSetLayoutBinding::default().binding(0).descriptor_type(vk::DescriptorType::UNIFORM_BUFFER).descriptor_count(1).stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default().binding(1).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).descriptor_count(1).stage_flags(vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default().binding(2).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).descriptor_count(1).stage_flags(vk::ShaderStageFlags::FRAGMENT),
        ];
        let l = unsafe { device.create_descriptor_set_layout(&vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings), None) }.unwrap();
        let ps = [
            vk::DescriptorPoolSize::default().ty(vk::DescriptorType::UNIFORM_BUFFER).descriptor_count(32),
            vk::DescriptorPoolSize::default().ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).descriptor_count(64),
        ];
        // 2 базовых + запас на текстурные сеты
        let p = unsafe { device.create_descriptor_pool(&vk::DescriptorPoolCreateInfo::default().max_sets(32).pool_sizes(&ps), None) }.unwrap();
        let layouts = [l, l, l];
        let sets = unsafe { device.allocate_descriptor_sets(&vk::DescriptorSetAllocateInfo::default().descriptor_pool(p).set_layouts(&layouts)) }.unwrap();
        (l, p, sets[0], sets[1], sets[2])
    }

    fn mt(
        instance: &Instance, pd: vk::PhysicalDevice, bits: u32, props: vk::MemoryPropertyFlags,
    ) -> u32 {
        let mp = unsafe { instance.get_physical_device_memory_properties(pd) };
        for i in 0..mp.memory_type_count {
            if (bits & (1 << i)) != 0 && (mp.memory_types[i as usize].property_flags & props) == props {
                return i;
            }
        }
        panic!("No memory type");
    }

    fn create_vb(
        device: &Device, instance: &Instance, pd: vk::PhysicalDevice, data: &[f32],
    ) -> (vk::Buffer, vk::DeviceMemory, u32) {
        let sz = (data.len() * 4) as u64;
        let b = unsafe { device.create_buffer(
            &vk::BufferCreateInfo::default().size(sz).usage(vk::BufferUsageFlags::VERTEX_BUFFER).sharing_mode(vk::SharingMode::EXCLUSIVE),
            None,
        ) }.unwrap();
        let mr = unsafe { device.get_buffer_memory_requirements(b) };
        let mt = Self::mt(instance, pd, mr.memory_type_bits, vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT);
        let m = unsafe { device.allocate_memory(
            &vk::MemoryAllocateInfo::default().allocation_size(mr.size).memory_type_index(mt), None,
        ) }.unwrap();
        unsafe { device.bind_buffer_memory(b, m, 0) }.unwrap();
        unsafe {
            let p = device.map_memory(m, 0, sz, vk::MemoryMapFlags::empty()).unwrap();
            std::ptr::copy_nonoverlapping(data.as_ptr() as *const u8, p as *mut u8, sz as usize);
            device.unmap_memory(m);
        }
        (b, m, (data.len() / 11) as u32)
    }

    fn create_ib(
        device: &Device, instance: &Instance, pd: vk::PhysicalDevice, data: &[u32],
    ) -> (vk::Buffer, vk::DeviceMemory, u32) {
        let sz = (data.len() * 4) as u64;
        let b = unsafe { device.create_buffer(
            &vk::BufferCreateInfo::default().size(sz).usage(vk::BufferUsageFlags::INDEX_BUFFER).sharing_mode(vk::SharingMode::EXCLUSIVE),
            None,
        ) }.unwrap();
        let mr = unsafe { device.get_buffer_memory_requirements(b) };
        let mt = Self::mt(instance, pd, mr.memory_type_bits, vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT);
        let m = unsafe { device.allocate_memory(
            &vk::MemoryAllocateInfo::default().allocation_size(mr.size).memory_type_index(mt), None,
        ) }.unwrap();
        unsafe { device.bind_buffer_memory(b, m, 0) }.unwrap();
        unsafe {
            let p = device.map_memory(m, 0, sz, vk::MemoryMapFlags::empty()).unwrap();
            std::ptr::copy_nonoverlapping(data.as_ptr() as *const u8, p as *mut u8, sz as usize);
            device.unmap_memory(m);
        }
        (b, m, data.len() as u32)
    }

    fn create_ub(
        device: &Device, instance: &Instance, pd: vk::PhysicalDevice, ubo: &Uniforms,
    ) -> (vk::Buffer, vk::DeviceMemory) {
        let sz = std::mem::size_of::<Uniforms>() as u64;
        let b = unsafe { device.create_buffer(
            &vk::BufferCreateInfo::default().size(sz).usage(vk::BufferUsageFlags::UNIFORM_BUFFER).sharing_mode(vk::SharingMode::EXCLUSIVE),
            None,
        ) }.unwrap();
        let mr = unsafe { device.get_buffer_memory_requirements(b) };
        let mt = Self::mt(instance, pd, mr.memory_type_bits, vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT);
        let m = unsafe { device.allocate_memory(
            &vk::MemoryAllocateInfo::default().allocation_size(mr.size).memory_type_index(mt), None,
        ) }.unwrap();
        unsafe { device.bind_buffer_memory(b, m, 0) }.unwrap();
        unsafe {
            let p = device.map_memory(m, 0, sz, vk::MemoryMapFlags::empty()).unwrap();
            std::ptr::copy_nonoverlapping(ubo as *const _ as *const u8, p as *mut u8, sz as usize);
            device.unmap_memory(m);
        }
        (b, m)
    }

    pub fn set_camera(&mut self, eye: Vec3, target: Vec3) { self.eye = eye; self.target = target; }
    pub fn set_wireframe(&mut self, on: bool) { self.wireframe = on; }
    pub fn set_thermal(&mut self, on: bool) { self.thermal_mode = on; }

    pub fn set_background(&mut self, color: crate::types::Color) {
        self.background = [color.r, color.g, color.b];
    }

    fn update_ubo(&self, buf: vk::Buffer, mem: vk::DeviceMemory, ubo: &Uniforms) {
        let sz = std::mem::size_of::<Uniforms>() as u64;
        unsafe { let p = self.device.map_memory(mem, 0, sz, vk::MemoryMapFlags::empty()).unwrap(); std::ptr::copy_nonoverlapping(ubo as *const _ as *const u8, p as *mut u8, sz as usize); self.device.unmap_memory(mem); }
    }

    fn update_buf(&self, _buf: vk::Buffer, mem: vk::DeviceMemory, data: &[u8]) {
        if data.is_empty() { return; }
        let sz = data.len() as u64;
        unsafe { let p = self.device.map_memory(mem, 0, sz, vk::MemoryMapFlags::empty()).unwrap(); std::ptr::copy_nonoverlapping(data.as_ptr(), p as *mut u8, data.len()); self.device.unmap_memory(mem); }
    }

    fn rebuild_bufs(&mut self, verts: &[f32], idxs: &[u32]) {
        let vsz = (verts.len() * 4) as u64;
        let isz = (idxs.len() * 4) as u64;
        if vsz != self.current_buffer_size {
            unsafe {
                self.device.destroy_buffer(self.vertex_buffer, None);
                self.device.free_memory(self.vertex_buffer_memory, None);
            }
            let (vb, vm, vc) = Self::create_vb(&self.device, &self.instance, self.physical_device, verts);
            self.vertex_buffer = vb; self.vertex_buffer_memory = vm; self.vertex_count = vc; self.current_buffer_size = vsz;
        } else {
            let vert_bytes = unsafe { std::slice::from_raw_parts(verts.as_ptr() as *const u8, verts.len() * 4) };
            self.update_buf(self.vertex_buffer, self.vertex_buffer_memory, vert_bytes);
        }
        if isz != self.current_index_buffer_size {
            unsafe { self.device.destroy_buffer(self.index_buffer, None); self.device.free_memory(self.index_buffer_memory, None); }
            let (ib, im, ic) = Self::create_ib(&self.device, &self.instance, self.physical_device, idxs);
            self.index_buffer = ib; self.index_buffer_memory = im; self.index_count = ic; self.current_index_buffer_size = isz;
        } else {
            let idx_bytes = unsafe { std::slice::from_raw_parts(idxs.as_ptr() as *const u8, idxs.len() * 4) };
            self.update_buf(self.index_buffer, self.index_buffer_memory, idx_bytes);
        }
    }

    fn rebuild_edge_bufs(&mut self, verts: &[f32], idxs: &[u32]) {
        if verts.is_empty() || idxs.is_empty() { return; }
        let vsz = (verts.len() * 4) as u64; let isz = (idxs.len() * 4) as u64;
        if vsz != self.current_edge_buffer_size {
            if self.current_edge_buffer_size > 0 { unsafe { self.device.destroy_buffer(self.edge_vertex_buffer, None); self.device.free_memory(self.edge_vertex_buffer_memory, None); } }
            let (vb, vm, vc) = Self::create_vb(&self.device, &self.instance, self.physical_device, verts);
            self.edge_vertex_buffer = vb; self.edge_vertex_buffer_memory = vm; self.edge_vertex_count = vc; self.current_edge_buffer_size = vsz;
        } else {
            let vert_bytes = unsafe { std::slice::from_raw_parts(verts.as_ptr() as *const u8, verts.len() * 4) };
            self.update_buf(self.edge_vertex_buffer, self.edge_vertex_buffer_memory, vert_bytes);
        }
        if isz != self.current_edge_index_buffer_size {
            if self.current_edge_index_buffer_size > 0 { unsafe { self.device.destroy_buffer(self.edge_index_buffer, None); self.device.free_memory(self.edge_index_buffer_memory, None); } }
            let (ib, im, ic) = Self::create_ib(&self.device, &self.instance, self.physical_device, idxs);
            self.edge_index_buffer = ib; self.edge_index_buffer_memory = im; self.edge_index_count = ic; self.current_edge_index_buffer_size = isz;
        } else {
            let idx_bytes = unsafe { std::slice::from_raw_parts(idxs.as_ptr() as *const u8, idxs.len() * 4) };
            self.update_buf(self.edge_index_buffer, self.edge_index_buffer_memory, idx_bytes);
        }
    }

    pub fn render_scene(&mut self, scene: &Scene, window_w: u32, window_h: u32, hover: u8, dt: f32) {
        if window_w == 0 || window_h == 0 { return; }
        unsafe {
            if let Ok(caps) = self.surface_loader.get_physical_device_surface_capabilities(self.physical_device, self.surface) {
                if caps.current_extent.width == 0 || caps.current_extent.height == 0 { return; }
            }
        }
        unsafe { self.device.wait_for_fences(std::slice::from_ref(&self.fence), true, u64::MAX).unwrap(); self.device.reset_fences(std::slice::from_ref(&self.fence)).unwrap(); }
        if window_w != self.window_width || window_h != self.window_height {
            self.rebuild_swapchain(window_w, window_h);
        }
        let image_index = match unsafe { self.swapchain_loader.acquire_next_image(self.swapchain, u64::MAX, self.image_available_semaphore, self.fence) } {
            Ok((idx, _)) => idx, Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => { self.rebuild_swapchain(window_w, window_h); return; } Err(_) => return,
        };
        unsafe { self.device.wait_for_fences(std::slice::from_ref(&self.fence), true, u64::MAX).unwrap(); self.device.reset_fences(std::slice::from_ref(&self.fence)).unwrap(); }

        // Build vertices: pos3 + color3 + uv2 = 8 floats per vertex
        // Каждый объект со своей текстурой — отдельный draw call.
        let mut all_v = Vec::new();
        let mut all_i = Vec::new();
        let mut edge_v = Vec::new();
        let mut edge_i = Vec::new();
        // (index_offset, index_count, descriptor_set)
        let mut draws: Vec<(u32, u32, vk::DescriptorSet)> = Vec::new();
        let mut default_set = self.descriptor_set_scene;

        // ---- Лёгкий view-frustum culling (границы камеры) ----
        let objs = scene.get_objects();
        let objs_len = objs.len();
        let mut visible = vec![true; objs_len];
        if objs_len > 0 {
            let sww = self.swapchain_extent.width.max(1) as f32;
            let shh = self.swapchain_extent.height.max(1) as f32;
            let proj = Mat4::perspective(45.0_f32.to_radians(), sww / shh, 0.1, 100.0);
            let view = Mat4::look_at(self.eye, self.target, Vec3::new(0.0, 1.0, 0.0));
            let vp = Mat4::multiply(&proj, &view);
            let planes = frustum_planes(&vp);
            for (i, obj) in objs.iter().enumerate() {
                let (wa, wb) = world_aabb(obj.aabb_min, obj.aabb_max, &obj.transform.model);
                if aabb_outside_frustum(&wa, &wb, &planes) { visible[i] = false; }
            }
        }

        let mut glows: Vec<([f32; 3], f32, f32)> = Vec::new();
        let mut eff_temps: Vec<f32> = Vec::new();
        let mut min_t = f32::MAX;
        let mut max_t = f32::MIN;
        if self.thermal_mode {
            let n = objs.len();
            if self.heat_state.len() != n {
                self.heat_state = objs.iter().map(|o| o.temp).collect();
            }
let mut cx = vec![0.0f32; n]; let mut cy = vec![0.0f32; n]; let mut cz = vec![0.0f32; n];
            let mut crad = vec![0.15f32; n];
            for (i, obj) in objs.iter().enumerate() {
                let (wa, wb) = world_aabb(obj.aabb_min, obj.aabb_max, &obj.transform.model);
                cx[i] = (wa[0] + wb[0]) * 0.5; cy[i] = (wa[1] + wb[1]) * 0.5; cz[i] = (wa[2] + wb[2]) * 0.5;
                let dx = wb[0] - wa[0]; let dy = wb[1] - wa[1]; let dz = wb[2] - wa[2];
                crad[i] = (dx * dx + dy * dy + dz * dz).sqrt() * 0.5;
            }
            // Динамическая диффузия тепла во времени (dt в секундах). Каждый объект
            // имеет СВОЙ радиус влияния: горячий объект греет соседей в радиусе
            // суммы их радиусов, и тепло так передаётся «дальше» по цепочке объектов.
            const CONDUCT: f32 = 0.5;   // проводимость, 1/сек
            const RESTORE: f32 = 0.08;  // возврат к источнику, 1/сек
            let dts = dt.clamp(0.0, 0.1);
            let con = CONDUCT * dts;
            let res = RESTORE * dts;
            let mut next: Vec<f32> = Vec::with_capacity(n);
            for i in 0..n {
                let base = objs[i].temp;
                let mut t = self.heat_state[i];
                for j in 0..n {
                    if j == i { continue; }
                    let dx = cx[i] - cx[j]; let dy = cy[i] - cy[j]; let dz = cz[i] - cz[j];
                    let d2 = dx * dx + dy * dy + dz * dz;
                    let d = d2.sqrt();
                    let reach = (crad[i] + crad[j]) * 2.0;
                    if reach < 1e-4 || d >= reach { continue; }
                    let mut infl = 1.0 - d / reach;
                    infl = infl * infl;
                    t += (self.heat_state[j] - self.heat_state[i]) * con * infl;
                }
                t += (base - t) * res;
                next.push(t);
                min_t = min_t.min(t); max_t = max_t.max(t);
            }
            self.heat_state = next;
            eff_temps = self.heat_state.clone();
            if max_t - min_t < 1e-6 { max_t = min_t + 1.0; }
        }

        for (oi, obj) in objs.iter().enumerate() {
            if !visible[oi] { continue; }
            if self.thermal_mode {
                let ti = eff_temps[oi];
                let n = if max_t > min_t { ((ti - min_t) / (max_t - min_t)).clamp(0.0, 1.0) } else { 0.0 };
                let s = ((n - 0.02) / 0.98).clamp(0.0, 1.0);
                let g = s * s * (3.0 - 2.0 * s);
                if g > 0.02 {
                    let (wa, wb) = world_aabb(obj.aabb_min, obj.aabb_max, &obj.transform.model);
                    let cx = (wa[0] + wb[0]) * 0.5; let cy = (wa[1] + wb[1]) * 0.5; let cz = (wa[2] + wb[2]) * 0.5;
                    let dx = wb[0] - wa[0]; let dy = wb[1] - wa[1]; let dz = wb[2] - wa[2];
                    let radius = (dx * dx + dy * dy + dz * dz).sqrt() * 0.5;
                    glows.push(([cx, cy, cz], radius.max(0.15), g));
                }
            }
            let m = &obj.mesh;
            let vs = &m.vertices;
            let is = &m.indices;
            let c: [f32; 3] = if self.thermal_mode {
                temp_to_color(eff_temps[oi], min_t, max_t)
            } else {
                obj.color.as_array()
            };
            let uvs = &m.uvs;
            let has_uv = m.has_uv && uvs.len() >= vs.len() / 3 * 2;
            let nt = normal_transform(&obj.transform.model);
            let norms = &m.normals;

            // Текстура объекта: если задана — грузим/берём из кэша
            let mut obj_set = default_set;
            let mut tex_aspect = 1.0f32;
            if let Some(ref tex) = m.texture {
                let (ds, aspect) = self.get_or_create_texture(tex);
                obj_set = ds;
                tex_aspect = aspect;
            }

            // Fill: центрируем UV, масштабируем по аспекту текстуры (cover)
            for (vi, ch) in vs.chunks(3).enumerate() {
                let tv = obj.transform.model.transform(&Vec3::new(ch[0], ch[1], ch[2]));
                all_v.push(tv.x);
                all_v.push(tv.y);
                all_v.push(tv.z);
                all_v.extend_from_slice(&c);
                let raw_n = if vi * 3 + 2 < norms.len() {
                    [norms[vi * 3], norms[vi * 3 + 1], norms[vi * 3 + 2]]
                } else {
                    [0.0, 0.0, 1.0]
                };
                let n = apply_normal(&raw_n, &nt);
                // В термальном режиме объекты светятся сами (без диффуза/теней/бликов):
                // нулевая нормаль => шейдер рендерит unlit, как термальное пятно.
                if self.thermal_mode {
                    all_v.extend_from_slice(&[0.0, 0.0, 0.0]);
                } else {
                    all_v.extend_from_slice(&n);
                }
                if has_uv {
                    let uvi = vi * 2;
                    let mut u = uvs[uvi];
                    let mut v = uvs[uvi + 1];
                    if m.texture_mode == TextureMode::Fill {
                        // cover: масштабируем так, чтобы текстура заполнила грань без искажений
                        if tex_aspect > 1.0 {
                            u = u * 1.0 / tex_aspect + (1.0 - 1.0 / tex_aspect) * 0.5;
                        } else {
                            v = v * tex_aspect + (1.0 - tex_aspect) * 0.5;
                        }
                    }
                    all_v.push(u);
                    all_v.push(v);
                } else {
                    all_v.push(0.0);
                    all_v.push(0.0);
                }
            }

            // Edge wireframe: same format (8 floats)
            if is.len() >= 3 {
                use std::collections::HashSet;
                let mut seen: HashSet<(u32, u32)> = HashSet::new();
                for tri in is.chunks(3) {
                    if tri.len() < 3 { break; }
                    let edges = [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])];
                    for &(a, b) in &edges {
                        let key = if a < b { (a, b) } else { (b, a) };
                        if seen.insert(key) {
                            let va = &vs[(a as usize) * 3..];
                            let vb = &vs[(b as usize) * 3..];
                            let pa = obj.transform.model.transform(&Vec3::new(va[0], va[1], va[2]));
                            let pb = obj.transform.model.transform(&Vec3::new(vb[0], vb[1], vb[2]));
                            edge_v.extend_from_slice(&[pa.x, pa.y, pa.z, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
                            edge_v.extend_from_slice(&[pb.x, pb.y, pb.z, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
                            let ei = (edge_v.len() / 11 - 2) as u32;
                            edge_i.push(ei);
                            edge_i.push(ei + 1);
                        }
                    }
                }
            }

            // Adjust indices
            let vert_offset = all_v.len() / 11 - (vs.len() / 3);
            let idx_start = all_i.len() as u32;
            for &ix in is {
                all_i.push(ix + vert_offset as u32);
            }
            let idx_count = is.len() as u32;
            draws.push((idx_start, idx_count, obj_set));
        }

        let scene_ic = all_i.len() as u32;
        let edge_ic = edge_i.len() as u32;
        // если сцена пустая — использовать дефолтный сет чтобы не ловить undefined
        if draws.is_empty() {
            draws.push((0, 0, default_set));
        }

        // Overlay (with dummy UV = 0,0)
        let w = self.swapchain_extent.width.max(1) as f32;
        let h = self.swapchain_extent.height.max(1) as f32;
        let ui = crate::render::ui::layout(w, h);
        let cx13 = ui.corner_x; let cy13 = ui.corner_y;
        let btn_rx = ui.btn_radius_x; let btn_ry = ui.btn_radius_y;
        let panel_left = ui.panel_left; let panel_right = ui.panel_right;
        let panel_bottom = ui.panel_bottom; let panel_top = ui.panel_top;
        let btn_left = ui.btn_left; let btn_right = ui.btn_right;
        let red_bottom = ui.close_bottom; let red_top = ui.close_top;
        let yel_bottom = ui.minimize_bottom; let yel_top = ui.minimize_top;
        let zd = 0.0;

        let (y_r, y_g, y_b) = if hover == 1 { (1.0, 0.95, 0.4) } else { (1.0, 0.882, 0.0) };
        let (r_r, r_g, r_b) = if hover == 2 { (1.0, 0.3, 0.3) } else { (1.0, 0.0, 0.0) };
        let segs = 20u32;
        let verts_per = (4 * segs + 1) as u32;

        macro_rules! push_rounded {
            ($v:expr, $i:expr, $base:expr, $left:expr, $bottom:expr, $right:expr, $top:expr, $rx:expr, $ry:expr, $r:expr, $g:expr, $b:expr) => {{
                let b = $base;
                let step = (std::f32::consts::PI / 2.0) / segs as f32;
                let arcs: [(f32, f32, f32); 4] = [
                    ($left + $rx, $bottom + $ry, std::f32::consts::PI),
                    ($right - $rx, $bottom + $ry, 3.0 * std::f32::consts::PI / 2.0),
                    ($right - $rx, $top - $ry, 0.0),
                    ($left + $rx, $top - $ry, std::f32::consts::PI / 2.0),
                ];
                for &(ccx, ccy, start) in &arcs {
                    for s in 0..segs {
                        let a = start + step * s as f32;
                        $v.extend_from_slice(&[ccx + $rx * a.cos(), ccy + $ry * a.sin(), zd, $r, $g, $b, 0.0, 0.0, 0.0, 0.0, 0.0]);
                    }
                }
                let perim = 4u32 * segs;
                let ctr = b + perim;
                $v.extend_from_slice(&[($left + $right) * 0.5, ($bottom + $top) * 0.5, zd, $r, $g, $b, 0.0, 0.0, 0.0, 0.0, 0.0]);
                for k in 0..perim {
                    let n = if k + 1 < perim { k + 1 } else { 0 };
                    $i.extend_from_slice(&[ctr, b + k, b + n]);
                }
            }};
        }

        macro_rules! push_rect {
            ($v:expr, $i:expr, $base:expr, $x1:expr, $y1:expr, $x2:expr, $y2:expr, $r:expr, $g:expr, $b:expr) => {{
                let b = $base;
                $v.extend_from_slice(&[$x1, $y1, zd, $r,$g,$b,0.0,0.0,0.0,0.0,0.0, $x2,$y1,zd,$r,$g,$b,0.0,0.0,0.0,0.0,0.0, $x2,$y2,zd,$r,$g,$b,0.0,0.0,0.0,0.0,0.0, $x1,$y2,zd,$r,$g,$b,0.0,0.0,0.0,0.0,0.0]);
                $i.extend_from_slice(&[b,b+1,b+2, b,b+2,b+3]);
            }};
        }

        let ov_base = (all_v.len() / 11) as u32;
        push_rounded!(all_v, all_i, ov_base, panel_left, panel_bottom, panel_right, panel_top, cx13, cy13, 0.580, 0.580, 0.580);
        let b2 = ov_base + verts_per;
        push_rounded!(all_v, all_i, b2, btn_left, red_bottom, btn_right, red_top, btn_rx, btn_ry, r_r, r_g, r_b);
        let b3 = b2 + verts_per;
        push_rounded!(all_v, all_i, b3, btn_left, yel_bottom, btn_right, yel_top, btn_rx, btn_ry, y_r, y_g, y_b);

        // Билборды-квады сияния (тепловизор): радиоцентр глоу, аддитивный бленд.
        let mut glow_start = all_i.len() as u32;
        if self.thermal_mode && !glows.is_empty() && self.glow_tex.is_some() {
            let eye = self.eye;
            let up = Vec3::new(0.0, 1.0, 0.0);
            for &(center, radius, g) in &glows {
                let c = Vec3::new(center[0], center[1], center[2]);
                let fwd = (c - eye).normalize();
                let right = fwd.cross(&up).normalize();
                let up2 = right.cross(&fwd).normalize();
                // Два кольца bloom: внешнее тёплое (широкий ореол) + внутреннее белое ядро.
                let rings: [(f32, [f32; 3]); 2] = [
                    (radius * (1.7 + 2.4 * g), [1.0, 0.55 + 0.45 * g, 0.18 + 0.82 * g]),
                    (radius * (0.85 + 1.1 * g), [1.0, 0.85 + 0.15 * g, 0.8 + 0.2 * g]),
                ];
                for (half, col) in rings {
                    let bl = c - right * half - up2 * half;
                    let br = c + right * half - up2 * half;
                    let tr = c + right * half + up2 * half;
                    let tl = c - right * half + up2 * half;
                    let base = (all_v.len() / 11) as u32;
                    for (p, u, v) in [(bl, 0.0, 0.0), (br, 1.0, 0.0), (tr, 1.0, 1.0), (tl, 0.0, 1.0)] {
                        all_v.extend_from_slice(&[p.x, p.y, p.z, col[0], col[1], col[2], 0.0, 0.0, 0.0, u, v]);
                    }
                    all_i.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
                }
            }
        }
        let glow_count = all_i.len() as u32 - glow_start;
        if self.thermal_mode && !self.glow_debug_done {
            self.glow_debug_done = true;
            println!("[Thermal] glow quads: {} objects, {} indices", glows.len(), glow_count);
        }

        let aspect = self.swapchain_extent.width as f32 / self.swapchain_extent.height as f32;
        self.uniforms.projection = Mat4::perspective(45.0_f32.to_radians(), aspect, 0.1, 100.0);
        self.uniforms.view = Mat4::look_at(self.eye, self.target, Vec3::new(0.0, 1.0, 0.0));
        self.uniforms.model = Mat4::identity();
        self.uniforms.view_pos = [self.eye.x, self.eye.y, self.eye.z, 1.0];

        // Матрица вида света (source -> сцена) + проекция для карты теней.
        // x = базовый bias, y = slope-коэффициент bias, z = толщина полосы PCF,
        // w = переключатель пасса теней (0 для основного прохода).
        let light_eye = Vec3::new(self.uniforms.light_pos[0], self.uniforms.light_pos[1], self.uniforms.light_pos[2]);
        let light_view = Mat4::look_at(light_eye, Vec3::zero(), Vec3::new(0.0, 1.0, 0.0));
        let light_proj = Mat4::perspective(85.0_f32.to_radians(), 1.0, 0.3, 200.0);
        self.uniforms.light_matrix = Mat4::multiply(&light_proj, &light_view);
        self.uniforms.shadow_params = [self.shadow_bias, 0.4, 0.003, 0.0];
        self.update_ubo(self.uniform_buffer, self.uniform_buffer_memory, &self.uniforms);
        // Карта теней: тот же сценовый UBO, но shadow_params.w=1 — основной
        // шейдер тогда проецирует геометрию в пространство света и пишет цвет
        // константой (только глубина идёт в карту теней) в отдельный буфер.
        let mut ubo_shadow = self.uniforms;
        ubo_shadow.shadow_params[3] = 1.0;
        self.update_ubo(self.shadow_uniform_buffer, self.shadow_uniform_buffer_memory, &ubo_shadow);
        self.uniforms_ov.projection = Mat4::orthographic(-1.0, 1.0, -1.0, 1.0, 0.0, 10.0);
        self.uniforms_ov.view = Mat4::identity();
        self.uniforms_ov.model = Mat4::identity();
        self.update_ubo(self.uniform_buffer_ov, self.uniform_buffer_ov_memory, &self.uniforms_ov);

        self.rebuild_bufs(&all_v, &all_i);

        // Record command buffer for this image
        let cmd = self.command_buffers[image_index as usize];
        unsafe {
            self.device.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty()).unwrap();
            self.device.begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default()).unwrap();

            // ===== Pass 1: карта теней (depth-only) =====
            let shadow_enabled = true;
            if shadow_enabled {
            let smr = vk::ImageSubresourceRange::default().aspect_mask(vk::ImageAspectFlags::DEPTH).base_mip_level(0).level_count(1).base_array_layer(0).layer_count(1);
            let to_dep = vk::ImageMemoryBarrier::default().image(self.shadow_map).subresource_range(smr).old_layout(vk::ImageLayout::UNDEFINED).new_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL).src_access_mask(vk::AccessFlags::empty()).dst_access_mask(vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE);
            self.device.cmd_pipeline_barrier(cmd, vk::PipelineStageFlags::TOP_OF_PIPE, vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS, vk::DependencyFlags::empty(), &[], &[], &[to_dep]);
            let scc = vk::ClearColorValue { float32: [0.0, 0.0, 0.0, 1.0] };
            let scd = vk::ClearDepthStencilValue { depth: 1.0, stencil: 0 };
            let scv = [vk::ClearValue { color: scc }, vk::ClearValue { depth_stencil: scd }];
            let s_ext = vk::Extent2D { width: self.shadow_map_size, height: self.shadow_map_size };
            let srpbi = vk::RenderPassBeginInfo::default().render_pass(self.shadow_render_pass).framebuffer(self.shadow_framebuffer).render_area(vk::Rect2D::default().offset(vk::Offset2D{x:0,y:0}).extent(s_ext)).clear_values(&scv);
            self.device.cmd_begin_render_pass(cmd, &srpbi, vk::SubpassContents::INLINE);
            let svp = vk::Viewport::default().x(0.0).y(0.0).width(s_ext.width as f32).height(s_ext.height as f32).min_depth(0.0).max_depth(1.0);
            let ssc = vk::Rect2D::default().offset(vk::Offset2D{x:0,y:0}).extent(s_ext);
            self.device.cmd_set_viewport(cmd, 0, &[svp]);
            self.device.cmd_set_scissor(cmd, 0, &[ssc]);
            self.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline.shadow_handle);
            self.device.cmd_bind_descriptor_sets(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline_layout, 0, std::slice::from_ref(&self.descriptor_set_shadow), &[]);
            self.device.cmd_bind_vertex_buffers(cmd, 0, &[self.vertex_buffer], &[0]);
            self.device.cmd_bind_index_buffer(cmd, self.index_buffer, 0, vk::IndexType::UINT32);
            // Вся геометрия сцены лежит непрерывно в [0, scene_ic); оверлей туда не входит.
            if scene_ic > 0 { self.device.cmd_draw_indexed(cmd, scene_ic, 1, 0, 0, 0); }
            self.device.cmd_end_render_pass(cmd);
            // Карта теней -> режим чтения для основного прохода
            let to_read = vk::ImageMemoryBarrier::default().image(self.shadow_map).subresource_range(smr).old_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL).new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL).src_access_mask(vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE).dst_access_mask(vk::AccessFlags::SHADER_READ);
            self.device.cmd_pipeline_barrier(cmd, vk::PipelineStageFlags::LATE_FRAGMENT_TESTS, vk::PipelineStageFlags::FRAGMENT_SHADER, vk::DependencyFlags::empty(), &[], &[], &[to_read]);
            }

            // ===== Pass 2: основной проход =====
            let bgk: f32 = if self.thermal_mode { 0.09 } else { 1.0 };
            let cc = vk::ClearColorValue { float32: [self.background[0] * bgk, self.background[1] * bgk, self.background[2] * bgk, 1.0] };
            let cd = vk::ClearDepthStencilValue { depth: 1.0, stencil: 0 };
            // Для MSAA рендер-пасс имеет 3 аттачмента (msaaColor, resolve, msaaDepth) →
            // clearValues должно быть 3; для остальных — 2 (color, depth).
            let cv: Vec<vk::ClearValue> = if self.aa_samples > 1 {
                vec![vk::ClearValue { color: cc }, vk::ClearValue { color: cc }, vk::ClearValue { depth_stencil: cd }]
            } else {
                vec![vk::ClearValue { color: cc }, vk::ClearValue { depth_stencil: cd }]
            };
            let rpbi = vk::RenderPassBeginInfo::default().render_pass(self.render_pass)
                .framebuffer(self.framebuffers[image_index as usize])
                .render_area(vk::Rect2D::default().offset(vk::Offset2D { x: 0, y: 0 }).extent(self.swapchain_extent))
                .clear_values(&cv);

            self.device.cmd_begin_render_pass(cmd, &rpbi, vk::SubpassContents::INLINE);
            self.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline.handle);
            let vp = vk::Viewport::default().x(0.0).y(0.0)
                .width(self.swapchain_extent.width as f32).height(self.swapchain_extent.height as f32)
                .min_depth(0.0).max_depth(1.0);
            let sc = vk::Rect2D::default().offset(vk::Offset2D{x:0,y:0}).extent(self.swapchain_extent);
            self.device.cmd_set_viewport(cmd, 0, &[vp]);
            self.device.cmd_set_scissor(cmd, 0, &[sc]);
            self.device.cmd_bind_vertex_buffers(cmd, 0, &[self.vertex_buffer], &[0]);
            self.device.cmd_bind_index_buffer(cmd, self.index_buffer, 0, vk::IndexType::UINT32);

            // Draw each object with its own descriptor set (texture)
            for &(idx_start, idx_count, ds) in &draws {
                if idx_count == 0 { continue; }
                self.device.cmd_bind_descriptor_sets(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline_layout, 0, std::slice::from_ref(&ds), &[]);
                self.device.cmd_draw_indexed(cmd, idx_count, 1, idx_start, 0, 0);
            }

            if self.wireframe && edge_ic > 0 {
                self.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline.line_handle);
                self.device.cmd_bind_descriptor_sets(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline_layout, 0, std::slice::from_ref(&self.descriptor_set_scene), &[]);
                self.device.cmd_bind_vertex_buffers(cmd, 0, &[self.edge_vertex_buffer], &[0]);
                self.device.cmd_bind_index_buffer(cmd, self.edge_index_buffer, 0, vk::IndexType::UINT32);
                self.device.cmd_draw_indexed(cmd, edge_ic, 1, 0, 0, 0);
                self.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline.handle);
                self.device.cmd_bind_vertex_buffers(cmd, 0, &[self.vertex_buffer], &[0]);
                self.device.cmd_bind_index_buffer(cmd, self.index_buffer, 0, vk::IndexType::UINT32);
            }
            self.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.ui_pipeline);
            self.device.cmd_bind_descriptor_sets(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline_layout, 0, std::slice::from_ref(&self.descriptor_set_ov), &[]);
            let ov_ic = glow_start - scene_ic;
            if ov_ic > 0 { self.device.cmd_draw_indexed(cmd, ov_ic, 1, scene_ic, 0, 0); }

            // Глоу-сияние (тепловизор): аддитивные билборды поверх тёмного фона
            if glow_count > 0 && self.additive_pipeline != vk::Pipeline::null() {
                if let Some((_, _, _, _, gds)) = self.glow_tex {
                    self.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.additive_pipeline);
                    self.device.cmd_bind_descriptor_sets(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline_layout, 0, std::slice::from_ref(&gds), &[]);
                    self.device.cmd_draw_indexed(cmd, glow_count, 1, glow_start, 0, 0);
                }
            }
            self.device.cmd_end_render_pass(cmd);

// ===== Pass 3: FXAA (только режим fxaa): scene_color → swapchain =====
            if self.use_fxaa {
                let scene_img = self.scene_color.unwrap();
                let swap_img = self.swapchain_images[image_index as usize];
                if let Some(pp) = self.present_pipeline {
                    let bar = vk::ImageMemoryBarrier::default().image(scene_img)
                        .subresource_range(vk::ImageSubresourceRange::default().aspect_mask(vk::ImageAspectFlags::COLOR).base_mip_level(0).level_count(1).base_array_layer(0).layer_count(1))
                        .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL).new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                        .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE).dst_access_mask(vk::AccessFlags::SHADER_READ);
                    self.device.cmd_pipeline_barrier(cmd, vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT, vk::PipelineStageFlags::FRAGMENT_SHADER, vk::DependencyFlags::empty(), &[], &[], &[bar]);
                    let prbi = vk::RenderPassBeginInfo::default().render_pass(self.present_render_pass.unwrap())
                        .framebuffer(self.present_framebuffers[image_index as usize])
                        .render_area(vk::Rect2D::default().offset(vk::Offset2D{x:0,y:0}).extent(self.swapchain_extent));
                    self.device.cmd_begin_render_pass(cmd, &prbi, vk::SubpassContents::INLINE);
                    self.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pp);
                    let pvp = vk::Viewport::default().x(0.0).y(0.0)
                        .width(self.swapchain_extent.width as f32).height(self.swapchain_extent.height as f32).min_depth(0.0).max_depth(1.0);
                    let psc = vk::Rect2D::default().offset(vk::Offset2D{x:0,y:0}).extent(self.swapchain_extent);
                    self.device.cmd_set_viewport(cmd, 0, &[pvp]);
                    self.device.cmd_set_scissor(cmd, 0, &[psc]);
                    self.device.cmd_bind_descriptor_sets(cmd, vk::PipelineBindPoint::GRAPHICS, self.present_layout.unwrap(), 0, std::slice::from_ref(&self.present_set.unwrap()), &[]);
                    self.device.cmd_bind_vertex_buffers(cmd, 0, &[self.present_vb], &[0]);
                    self.device.cmd_draw(cmd, 3, 1, 0, 0);
                    self.device.cmd_end_render_pass(cmd);
                } else {
                    // FXAA-пайплайн не поддержан драйвером → простой блур/копия.
                    let sub = vk::ImageSubresourceRange::default().aspect_mask(vk::ImageAspectFlags::COLOR).base_mip_level(0).level_count(1).base_array_layer(0).layer_count(1);
                    let b1 = vk::ImageMemoryBarrier::default().image(scene_img).subresource_range(sub).old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL).new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL).src_access_mask(vk::AccessFlags::SHADER_READ).dst_access_mask(vk::AccessFlags::TRANSFER_READ);
                    self.device.cmd_pipeline_barrier(cmd, vk::PipelineStageFlags::FRAGMENT_SHADER, vk::PipelineStageFlags::TRANSFER, vk::DependencyFlags::empty(), &[], &[], &[b1]);
                    let b2 = vk::ImageMemoryBarrier::default().image(swap_img).subresource_range(sub).old_layout(vk::ImageLayout::UNDEFINED).new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL).src_access_mask(vk::AccessFlags::empty()).dst_access_mask(vk::AccessFlags::TRANSFER_WRITE);
                    self.device.cmd_pipeline_barrier(cmd, vk::PipelineStageFlags::TOP_OF_PIPE, vk::PipelineStageFlags::TRANSFER, vk::DependencyFlags::empty(), &[], &[], &[b2]);
                    let ib = vk::ImageBlit::default().src_subresource(vk::ImageSubresourceLayers::default().aspect_mask(vk::ImageAspectFlags::COLOR).layer_count(1)).dst_subresource(vk::ImageSubresourceLayers::default().aspect_mask(vk::ImageAspectFlags::COLOR).layer_count(1)).src_offsets([vk::Offset3D{x:0,y:0,z:0}, vk::Offset3D{x:self.swapchain_extent.width as i32,y:self.swapchain_extent.height as i32,z:1}]).dst_offsets([vk::Offset3D{x:0,y:0,z:0}, vk::Offset3D{x:self.swapchain_extent.width as i32,y:self.swapchain_extent.height as i32,z:1}]);
                    self.device.cmd_blit_image(cmd, scene_img, vk::ImageLayout::TRANSFER_SRC_OPTIMAL, swap_img, vk::ImageLayout::TRANSFER_DST_OPTIMAL, &[ib], vk::Filter::NEAREST);
                    let b3 = vk::ImageMemoryBarrier::default().image(swap_img).subresource_range(sub).old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL).new_layout(vk::ImageLayout::PRESENT_SRC_KHR).src_access_mask(vk::AccessFlags::TRANSFER_WRITE).dst_access_mask(vk::AccessFlags::empty());
                    self.device.cmd_pipeline_barrier(cmd, vk::PipelineStageFlags::TRANSFER, vk::PipelineStageFlags::BOTTOM_OF_PIPE, vk::DependencyFlags::empty(), &[], &[], &[b3]);
                }
            }
            self.device.end_command_buffer(cmd).unwrap();
        }

        // Submit
        let si = vk::SubmitInfo::default()
            .wait_semaphores(std::slice::from_ref(&self.image_available_semaphore))
            .wait_dst_stage_mask(std::slice::from_ref(&vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT))
            .command_buffers(std::slice::from_ref(&cmd))
            .signal_semaphores(std::slice::from_ref(&self.render_finished_semaphore));
        let pi = vk::PresentInfoKHR::default()
            .wait_semaphores(std::slice::from_ref(&self.render_finished_semaphore))
            .swapchains(std::slice::from_ref(&self.swapchain))
            .image_indices(std::slice::from_ref(&image_index));

        unsafe {
            self.device.queue_submit(self.queue, std::slice::from_ref(&si), self.fence).unwrap();
            match self.swapchain_loader.queue_present(self.queue, &pi) {
                Ok(true) | Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => { self.rebuild_swapchain(window_w, window_h); }
                _ => {}
            }
        }
    }

    fn rebuild_swapchain(&mut self, w: u32, h: u32) {
        if w == 0 || h == 0 { return; }
        let (sc, fmt, ext, imgs) = match Self::create_swapchain(self.physical_device, &self.swapchain_loader, self.surface, &self.surface_loader, w, h) {
            Some(v) => v, None => return,
        };
        unsafe { self.device.queue_wait_idle(self.queue).unwrap(); }
        unsafe {
            for &fb in &self.framebuffers { self.device.destroy_framebuffer(fb, None); }
            for &fb in &self.present_framebuffers { self.device.destroy_framebuffer(fb, None); }
            for &view in &self.swapchain_image_views { self.device.destroy_image_view(view, None); }
            self.device.destroy_image_view(self.depth_image_view, None); self.device.destroy_image(self.depth_image, None); self.device.free_memory(self.depth_image_memory, None);
            if self.aa_samples > 1 {
                self.device.destroy_image_view(self.msaa_color_view, None); self.device.destroy_image(self.msaa_color, None); self.device.free_memory(self.msaa_color_mem, None);
                self.device.destroy_image_view(self.msaa_depth_view, None); self.device.destroy_image(self.msaa_depth, None); self.device.free_memory(self.msaa_depth_mem, None);
            }
            if self.use_fxaa {
                self.device.destroy_image_view(self.scene_color_view.unwrap(), None); self.device.destroy_image(self.scene_color.unwrap(), None); self.device.free_memory(self.scene_color_mem.unwrap(), None);
            }
            self.swapchain_loader.destroy_swapchain(self.swapchain, None);
        }
        self.window_width = w.max(1); self.window_height = h.max(1);
        self.swapchain = sc; self.swapchain_format = fmt; self.swapchain_extent = ext; self.swapchain_images = imgs;
        self.swapchain_image_views = self.swapchain_images.iter().map(|&img| unsafe { self.device.create_image_view(&vk::ImageViewCreateInfo::default().image(img).view_type(vk::ImageViewType::TYPE_2D).format(fmt).subresource_range(vk::ImageSubresourceRange::default().aspect_mask(vk::ImageAspectFlags::COLOR).base_mip_level(0).level_count(1).base_array_layer(0).layer_count(1)), None) }.unwrap()).collect();
        let (dimg, dimg_mem, dimg_view, dfmt) = Self::create_depth(&self.device, &self.instance, self.physical_device, ext.width, ext.height, 1);
        self.depth_image = dimg; self.depth_image_memory = dimg_mem; self.depth_image_view = dimg_view; self.depth_format = dfmt;
        let scenefmt = self.swapchain_format;
        if self.aa_samples > 1 {
            let (c, cm, cv) = Self::create_color_samples(&self.device, &self.instance, self.physical_device, scenefmt, ext.width, ext.height, self.aa_samples, false);
            self.msaa_color = c; self.msaa_color_mem = cm; self.msaa_color_view = cv;
            let (d, dm, dv, _) = Self::create_depth(&self.device, &self.instance, self.physical_device, ext.width, ext.height, self.aa_samples);
            self.msaa_depth = d; self.msaa_depth_mem = dm; self.msaa_depth_view = dv;
            self.framebuffers = self.swapchain_image_views.iter().map(|&view| { let att = [self.msaa_color_view, view, self.msaa_depth_view]; unsafe { self.device.create_framebuffer(&vk::FramebufferCreateInfo::default().render_pass(self.render_pass).attachments(&att).width(ext.width).height(ext.height).layers(1), None) }.unwrap() }).collect();
        } else if self.use_fxaa {
            let (c, cm, cv) = Self::create_color_samples(&self.device, &self.instance, self.physical_device, scenefmt, ext.width, ext.height, 1, true);
            self.scene_color = Some(c); self.scene_color_mem = Some(cm); self.scene_color_view = Some(cv);
            let att = [self.scene_color_view.unwrap(), self.depth_image_view];
            let fb = unsafe { self.device.create_framebuffer(&vk::FramebufferCreateInfo::default().render_pass(self.render_pass).attachments(&att).width(ext.width).height(ext.height).layers(1), None) }.unwrap();
            self.framebuffers = (0..self.swapchain_image_views.len()).map(|_| fb).collect();
            self.present_framebuffers = self.swapchain_image_views.iter().map(|&view| unsafe { self.device.create_framebuffer(&vk::FramebufferCreateInfo::default().render_pass(self.present_render_pass.unwrap()).attachments(&[view]).width(ext.width).height(ext.height).layers(1), None) }.unwrap()).collect();
            let sci = vk::DescriptorImageInfo::default().image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL).image_view(self.scene_color_view.unwrap()).sampler(self.present_sampler.unwrap());
            unsafe { self.device.update_descriptor_sets(&[vk::WriteDescriptorSet::default().dst_set(self.present_set.unwrap()).dst_binding(0).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&sci))], &[]); }
        } else {
            self.framebuffers = self.swapchain_image_views.iter().map(|&view| { let att = [view, self.depth_image_view]; unsafe { self.device.create_framebuffer(&vk::FramebufferCreateInfo::default().render_pass(self.render_pass).attachments(&att).width(ext.width).height(ext.height).layers(1), None) }.unwrap() }).collect();
        }
    }

    pub fn cleanup(&mut self) {
        unsafe {
            self.device.queue_wait_idle(self.queue).unwrap();
            self.pipeline.cleanup(&self.device);
            if self.ui_pipeline != vk::Pipeline::null() { self.device.destroy_pipeline(self.ui_pipeline, None); }
            if self.additive_pipeline != vk::Pipeline::null() { self.device.destroy_pipeline(self.additive_pipeline, None); }
            if let Some((img, mem, view, sampler, _ds)) = self.glow_tex {
                self.device.destroy_sampler(sampler, None);
                self.device.destroy_image_view(view, None);
                self.device.destroy_image(img, None);
                self.device.free_memory(mem, None);
            }
            self.device.destroy_pipeline_layout(self.pipeline_layout, None);
            self.device.destroy_descriptor_pool(self.descriptor_pool, None);
            self.device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            self.device.destroy_framebuffer(self.shadow_framebuffer, None);
            self.device.destroy_render_pass(self.shadow_render_pass, None);
            self.device.destroy_sampler(self.shadow_map_sampler, None);
            self.device.destroy_image_view(self.shadow_map_view, None);
            self.device.destroy_image(self.shadow_map, None);
            self.device.free_memory(self.shadow_map_memory, None);
            self.device.destroy_image_view(self.shadow_color_view, None);
            self.device.destroy_image(self.shadow_color, None);
            self.device.free_memory(self.shadow_color_memory, None);
            self.device.destroy_buffer(self.vertex_buffer, None); self.device.free_memory(self.vertex_buffer_memory, None);
            self.device.destroy_buffer(self.index_buffer, None); self.device.free_memory(self.index_buffer_memory, None);
            self.device.destroy_buffer(self.edge_vertex_buffer, None); self.device.free_memory(self.edge_vertex_buffer_memory, None);
            self.device.destroy_buffer(self.edge_index_buffer, None); self.device.free_memory(self.edge_index_buffer_memory, None);
            self.device.destroy_buffer(self.uniform_buffer, None); self.device.free_memory(self.uniform_buffer_memory, None);
            self.device.destroy_buffer(self.uniform_buffer_ov, None); self.device.free_memory(self.uniform_buffer_ov_memory, None);
            self.device.destroy_buffer(self.shadow_uniform_buffer, None); self.device.free_memory(self.shadow_uniform_buffer_memory, None);
            self.device.destroy_buffer(self.present_vb, None); self.device.free_memory(self.present_vb_mem, None);
            // MSAA-цели
            if self.aa_samples > 1 {
                self.device.destroy_image_view(self.msaa_color_view, None); self.device.destroy_image(self.msaa_color, None); self.device.free_memory(self.msaa_color_mem, None);
                self.device.destroy_image_view(self.msaa_depth_view, None); self.device.destroy_image(self.msaa_depth, None); self.device.free_memory(self.msaa_depth_mem, None);
            }
            // FXAA-пост
            if self.use_fxaa {
                for &fb in &self.present_framebuffers { self.device.destroy_framebuffer(fb, None); }
                if let Some(p) = self.present_pipeline { self.device.destroy_pipeline(p, None); }
                if let Some(l) = self.present_layout { self.device.destroy_pipeline_layout(l, None); }
                if let Some(d) = self.present_dsl { self.device.destroy_descriptor_set_layout(d, None); }
                if let Some(s) = self.present_sampler { self.device.destroy_sampler(s, None); }
                if let Some(r) = self.present_render_pass { self.device.destroy_render_pass(r, None); }
                if let (Some(v), Some(im), Some(m)) = (self.scene_color_view, self.scene_color, self.scene_color_mem) {
                    self.device.destroy_image_view(v, None);
                    self.device.destroy_image(im, None);
                    self.device.free_memory(m, None);
                }
            }
            self.device.destroy_image_view(self.depth_image_view, None); self.device.destroy_image(self.depth_image, None); self.device.free_memory(self.depth_image_memory, None);
            self.device.destroy_sampler(self.texture_sampler, None);
            self.device.destroy_image_view(self.texture_image_view, None);
            self.device.destroy_image(self.texture_image, None);
            self.device.free_memory(self.texture_image_memory, None);
            self.device.destroy_fence(self.fence, None); self.device.destroy_semaphore(self.image_available_semaphore, None); self.device.destroy_semaphore(self.render_finished_semaphore, None);
            self.device.destroy_command_pool(self.command_pool, None); self.device.destroy_render_pass(self.render_pass, None);
            for &fb in &self.framebuffers { self.device.destroy_framebuffer(fb, None); }
            for &view in &self.swapchain_image_views { self.device.destroy_image_view(view, None); }
            self.swapchain_loader.destroy_swapchain(self.swapchain, None);
            self.device.destroy_device(None);
            self.surface_loader.destroy_surface(self.surface, None);
            self.instance.destroy_instance(None);
        }
    }
}