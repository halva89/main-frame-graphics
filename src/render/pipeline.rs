use ash::vk;
use std::ffi::CString;
use std::fs;

pub struct Pipeline {
    pub layout: vk::PipelineLayout,
    pub handle: vk::Pipeline,
    pub line_handle: vk::Pipeline,
    pub shadow_handle: vk::Pipeline,
}

pub fn samples_flags(samples: u32) -> vk::SampleCountFlags {
    match samples {
        2 => vk::SampleCountFlags::TYPE_2,
        4 => vk::SampleCountFlags::TYPE_4,
        8 => vk::SampleCountFlags::TYPE_8,
        _ => vk::SampleCountFlags::TYPE_1,
    }
}

impl Pipeline {
    pub fn new(device: &ash::Device, render_pass: vk::RenderPass, swapchain_extent: vk::Extent2D) -> Self {
        let layout = unsafe { device.create_pipeline_layout(&vk::PipelineLayoutCreateInfo::default(), None) }
            .expect("Failed to create pipeline layout");
        Self::new_with_layout(device, render_pass, swapchain_extent, layout, 1.0, 1)
    }

    pub fn new_with_layout(
        device: &ash::Device, 
        render_pass: vk::RenderPass, 
        swapchain_extent: vk::Extent2D,
        layout: vk::PipelineLayout,
        line_width: f32,
        samples: u32,
    ) -> Self {
        println!("[Pipeline] Creating pipeline... (line_width = {}, samples = {})", line_width, samples);
        
        let vert_path = "shaders/vert.spv";
        let frag_path = "shaders/frag.spv";
        
        let vert_code = Self::load_shader(vert_path);
        let frag_code = Self::load_shader(frag_path);

        let vert_module = Self::create_shader_module(device, &vert_code);
        let frag_module = Self::create_shader_module(device, &frag_code);

        let name = CString::new("main").unwrap();

        let vert_stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert_module)
            .name(&name);

        let frag_stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag_module)
            .name(&name);

        let stages = [vert_stage, frag_stage];

        let binding_desc = vk::VertexInputBindingDescription::default()
            .binding(0)
            .stride(11 * std::mem::size_of::<f32>() as u32)
            .input_rate(vk::VertexInputRate::VERTEX);

        let attr_descs = [
            vk::VertexInputAttributeDescription::default()
                .location(0)
                .binding(0)
                .format(vk::Format::R32G32B32_SFLOAT)
                .offset(0),
            vk::VertexInputAttributeDescription::default()
                .location(1)
                .binding(0)
                .format(vk::Format::R32G32B32_SFLOAT)
                .offset(3 * std::mem::size_of::<f32>() as u32),
            vk::VertexInputAttributeDescription::default()
                .location(2)
                .binding(0)
                .format(vk::Format::R32G32_SFLOAT)
                .offset(9 * std::mem::size_of::<f32>() as u32),
            vk::VertexInputAttributeDescription::default()
                .location(3)
                .binding(0)
                .format(vk::Format::R32G32B32_SFLOAT)
                .offset(6 * std::mem::size_of::<f32>() as u32),
        ];

        let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_binding_descriptions(std::slice::from_ref(&binding_desc))
            .vertex_attribute_descriptions(&attr_descs);

        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST);

        let viewport = vk::Viewport::default()
            .x(0.0)
            .y(0.0)
            .width(swapchain_extent.width as f32)
            .height(swapchain_extent.height as f32)
            .min_depth(0.0)
            .max_depth(1.0);

        let scissor = vk::Rect2D::default()
            .offset(vk::Offset2D { x: 0, y: 0 })
            .extent(swapchain_extent);

        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .viewports(std::slice::from_ref(&viewport))
            .scissors(std::slice::from_ref(&scissor));

        let rasterizer = vk::PipelineRasterizationStateCreateInfo::default()
            .polygon_mode(vk::PolygonMode::FILL)
            .line_width(line_width)
            .cull_mode(vk::CullModeFlags::NONE)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE);

        let multisample = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(samples_flags(samples));

        let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default()
            .depth_test_enable(true)
            .depth_write_enable(true)
            .depth_compare_op(vk::CompareOp::LESS);

        let color_blend_attachment = vk::PipelineColorBlendAttachmentState::default()
            .color_write_mask(vk::ColorComponentFlags::RGBA)
            .blend_enable(false);

        let color_blend = vk::PipelineColorBlendStateCreateInfo::default()
            .attachments(std::slice::from_ref(&color_blend_attachment));

        let fill_info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&stages)
            .vertex_input_state(&vertex_input)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterizer)
            .multisample_state(&multisample)
            .depth_stencil_state(&depth_stencil)
            .color_blend_state(&color_blend)
            .layout(layout)
            .render_pass(render_pass)
            .subpass(0);

        // Пайплайн каркаса (wireframe): та же геометрия, но линии.
        let line_input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::LINE_LIST);

        let line_rasterizer = vk::PipelineRasterizationStateCreateInfo::default()
            .polygon_mode(vk::PolygonMode::LINE)
            .line_width(line_width)
            .cull_mode(vk::CullModeFlags::NONE)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE);

        let line_info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&stages)
            .vertex_input_state(&vertex_input)
            .input_assembly_state(&line_input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&line_rasterizer)
            .multisample_state(&multisample)
            .depth_stencil_state(&depth_stencil)
            .color_blend_state(&color_blend)
            .layout(layout)
            .render_pass(render_pass)
            .subpass(0);

        let infos = [fill_info, line_info];
        let pipelines = unsafe {
            device.create_graphics_pipelines(vk::PipelineCache::null(), &infos, None)
        };

        unsafe {
            device.destroy_shader_module(vert_module, None);
            device.destroy_shader_module(frag_module, None);
        }

        match pipelines {
            Ok(p) => {
                println!("[Pipeline] Pipelines created successfully! (fill + line)");
                Self { layout, handle: p[0], line_handle: p[1], shadow_handle: vk::Pipeline::null() }
            }
            Err(e) => {
                panic!("Failed to create pipeline: {:?}", e);
            }
        }
    }

    // Отдельный single-sample пайплайн для пасса теней. Он обязан быть
    // MSAA-независимым (sample = 1), т.к. карта теней — обычный single-sample
    // рендер-пасс, а геометрия использует те же вершинные/индексные буферы.
    pub fn create_shadow(device: &ash::Device, shadow_render_pass: vk::RenderPass, layout: vk::PipelineLayout) -> vk::Pipeline {
        println!("[Pipeline] Creating shadow (single-sample) pipeline...");
        let name = CString::new("main").unwrap();
        let vc = Self::load_shader("shaders/vert.spv");
        let fc = Self::load_shader("shaders/frag.spv");
        let vm = Self::create_shader_module(device, &vc);
        let fm = Self::create_shader_module(device, &fc);
        let vs = vk::PipelineShaderStageCreateInfo::default().stage(vk::ShaderStageFlags::VERTEX).module(vm).name(&name);
        let fs = vk::PipelineShaderStageCreateInfo::default().stage(vk::ShaderStageFlags::FRAGMENT).module(fm).name(&name);
        let stages = [vs, fs];

        let binding_desc = vk::VertexInputBindingDescription::default().binding(0).stride(11 * std::mem::size_of::<f32>() as u32).input_rate(vk::VertexInputRate::VERTEX);
        let attr_descs = [
            vk::VertexInputAttributeDescription::default().location(0).binding(0).format(vk::Format::R32G32B32_SFLOAT).offset(0),
            vk::VertexInputAttributeDescription::default().location(1).binding(0).format(vk::Format::R32G32B32_SFLOAT).offset(3 * std::mem::size_of::<f32>() as u32),
            vk::VertexInputAttributeDescription::default().location(2).binding(0).format(vk::Format::R32G32_SFLOAT).offset(9 * std::mem::size_of::<f32>() as u32),
            vk::VertexInputAttributeDescription::default().location(3).binding(0).format(vk::Format::R32G32B32_SFLOAT).offset(6 * std::mem::size_of::<f32>() as u32),
        ];
        let vertex_input = vk::PipelineVertexInputStateCreateInfo::default().vertex_binding_descriptions(std::slice::from_ref(&binding_desc)).vertex_attribute_descriptions(&attr_descs);
        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default().topology(vk::PrimitiveTopology::TRIANGLE_LIST);
        let viewport = vk::Viewport::default().x(0.0).y(0.0).width(1.0).height(1.0).min_depth(0.0).max_depth(1.0);
        let scissor = vk::Rect2D::default().offset(vk::Offset2D { x: 0, y: 0 }).extent(vk::Extent2D { width: 1, height: 1 });
        let viewport_state = vk::PipelineViewportStateCreateInfo::default().viewports(std::slice::from_ref(&viewport)).scissors(std::slice::from_ref(&scissor));
        let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let dynamic_state = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
        let rasterizer = vk::PipelineRasterizationStateCreateInfo::default().polygon_mode(vk::PolygonMode::FILL).line_width(1.0).cull_mode(vk::CullModeFlags::NONE).front_face(vk::FrontFace::COUNTER_CLOCKWISE);
        let multisample = vk::PipelineMultisampleStateCreateInfo::default().rasterization_samples(vk::SampleCountFlags::TYPE_1);
        let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default().depth_test_enable(true).depth_write_enable(true).depth_compare_op(vk::CompareOp::LESS);
        let color_attachment = vk::PipelineColorBlendAttachmentState::default().color_write_mask(vk::ColorComponentFlags::RGBA).blend_enable(false);
        let color_blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(std::slice::from_ref(&color_attachment));
        let pinfo = vk::GraphicsPipelineCreateInfo::default().stages(&stages).vertex_input_state(&vertex_input).input_assembly_state(&input_assembly).viewport_state(&viewport_state).dynamic_state(&dynamic_state).rasterization_state(&rasterizer).multisample_state(&multisample).depth_stencil_state(&depth_stencil).color_blend_state(&color_blend).layout(layout).render_pass(shadow_render_pass).subpass(0);
        let result = unsafe { device.create_graphics_pipelines(vk::PipelineCache::null(), std::slice::from_ref(&pinfo), None) };
        unsafe { device.destroy_shader_module(vm, None); device.destroy_shader_module(fm, None); }
        match result {
            Ok(p) => { println!("[Pipeline] Shadow pipeline created"); p[0] }
            Err(e) => panic!("Failed to create shadow pipeline: {:?}", e),
        }
    }

    // UI-оверлей (панель окна): тот же шейдер, что и сцена, но без теста и
    // записи глубины — интерфейс всегда поверх геометрии, кнопки поверх панели.
    pub fn create_ui(device: &ash::Device, render_pass: vk::RenderPass, layout: vk::PipelineLayout, samples: u32) -> vk::Pipeline {
        println!("[Pipeline] Creating UI pipeline...");
        let name = CString::new("main").unwrap();
        let vc = Self::load_shader("shaders/vert.spv");
        let fc = Self::load_shader("shaders/frag.spv");
        let vm = Self::create_shader_module(device, &vc);
        let fm = Self::create_shader_module(device, &fc);
        let vs = vk::PipelineShaderStageCreateInfo::default().stage(vk::ShaderStageFlags::VERTEX).module(vm).name(&name);
        let fs = vk::PipelineShaderStageCreateInfo::default().stage(vk::ShaderStageFlags::FRAGMENT).module(fm).name(&name);
        let stages = [vs, fs];

        let binding_desc = vk::VertexInputBindingDescription::default().binding(0).stride(11 * std::mem::size_of::<f32>() as u32).input_rate(vk::VertexInputRate::VERTEX);
        let attr_descs = [
            vk::VertexInputAttributeDescription::default().location(0).binding(0).format(vk::Format::R32G32B32_SFLOAT).offset(0),
            vk::VertexInputAttributeDescription::default().location(1).binding(0).format(vk::Format::R32G32B32_SFLOAT).offset(3 * std::mem::size_of::<f32>() as u32),
            vk::VertexInputAttributeDescription::default().location(2).binding(0).format(vk::Format::R32G32_SFLOAT).offset(9 * std::mem::size_of::<f32>() as u32),
            vk::VertexInputAttributeDescription::default().location(3).binding(0).format(vk::Format::R32G32B32_SFLOAT).offset(6 * std::mem::size_of::<f32>() as u32),
        ];
        let vertex_input = vk::PipelineVertexInputStateCreateInfo::default().vertex_binding_descriptions(std::slice::from_ref(&binding_desc)).vertex_attribute_descriptions(&attr_descs);
        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default().topology(vk::PrimitiveTopology::TRIANGLE_LIST);
        let viewport = vk::Viewport::default().x(0.0).y(0.0).width(1.0).height(1.0).min_depth(0.0).max_depth(1.0);
        let scissor = vk::Rect2D::default().offset(vk::Offset2D { x: 0, y: 0 }).extent(vk::Extent2D { width: 1, height: 1 });
        let viewport_state = vk::PipelineViewportStateCreateInfo::default().viewports(std::slice::from_ref(&viewport)).scissors(std::slice::from_ref(&scissor));
        let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let dynamic_state = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
        let rasterizer = vk::PipelineRasterizationStateCreateInfo::default().polygon_mode(vk::PolygonMode::FILL).line_width(1.0).cull_mode(vk::CullModeFlags::NONE).front_face(vk::FrontFace::COUNTER_CLOCKWISE);
        let multisample = vk::PipelineMultisampleStateCreateInfo::default().rasterization_samples(samples_flags(samples));
        let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default().depth_test_enable(false).depth_write_enable(false);
        let cba = vk::PipelineColorBlendAttachmentState::default().color_write_mask(vk::ColorComponentFlags::RGBA).blend_enable(false);
        let color_blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(std::slice::from_ref(&cba));
        let pinfo = vk::GraphicsPipelineCreateInfo::default().stages(&stages).vertex_input_state(&vertex_input).input_assembly_state(&input_assembly).viewport_state(&viewport_state).dynamic_state(&dynamic_state).rasterization_state(&rasterizer).multisample_state(&multisample).depth_stencil_state(&depth_stencil).color_blend_state(&color_blend).layout(layout).render_pass(render_pass).subpass(0);
        let result = unsafe { device.create_graphics_pipelines(vk::PipelineCache::null(), std::slice::from_ref(&pinfo), None) };
        unsafe { device.destroy_shader_module(vm, None); device.destroy_shader_module(fm, None); }
        match result {
            Ok(p) => p[0],
            Err(e) => panic!("Failed to create UI pipeline: {:?}", e),
        }
    }

    fn load_shader(path: &str) -> Vec<u32> {
        let bytes = fs::read(path).expect(&format!("Failed to read shader file: {}", path));
        let code = bytes.chunks_exact(4)
            .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
            .collect::<Vec<_>>();
        
        code
}

    fn create_shader_module(device: &ash::Device, code: &[u32]) -> vk::ShaderModule {
        unsafe {
            device.create_shader_module(
                &vk::ShaderModuleCreateInfo::default().code(code),
                None,
            )
            .expect("Failed to create shader module")
        }
    }

    pub fn cleanup(&self, device: &ash::Device) {
        unsafe {
            device.destroy_pipeline(self.handle, None);
            device.destroy_pipeline(self.line_handle, None);
            if self.shadow_handle != vk::Pipeline::null() {
                device.destroy_pipeline(self.shadow_handle, None);
            }
        }
    }
}