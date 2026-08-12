use wgpu::util::DeviceExt;
use std::borrow::Cow;
use bytemuck::{Pod, Zeroable};

/// Monte Carlo Value-at-Risk (VaR) parameters for the GPU compute shader
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct RiskParams {
    pub initial_price: f32,
    pub volatility: f32,
    pub drift: f32,
    pub dt: f32,
    pub steps: u32,
    pub num_paths: u32,
    pub _padding1: u32,
    pub _padding2: u32,
}

pub struct GpuRiskEngine {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
}

impl GpuRiskEngine {
    /// Initializes the WebGPU context for hardware-accelerated risk modeling
    pub async fn new() -> anyhow::Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }).await.ok_or_else(|| anyhow::anyhow!("Failed to find an appropriate GPU adapter"))?;

        let (device, queue) = adapter.request_device(
            &wgpu::DeviceDescriptor {
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                label: Some("B-Terminal GPU Risk Compute Device"),
            },
            None,
        ).await?;

        // Inline WGSL compute shader for Geometric Brownian Motion
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Monte Carlo VaR Shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(r#"
                struct RiskParams {
                    initial_price: f32,
                    volatility: f32,
                    drift: f32,
                    dt: f32,
                    steps: u32,
                    num_paths: u32,
                    _padding1: u32,
                    _padding2: u32,
                };
                
                @group(0) @binding(0) var<uniform> params: RiskParams;
                @group(0) @binding(1) var<storage, read_write> results: array<f32>;
                
                // Very crude pseudo-random generator for the shader
                fn pcg_hash(input: u32) -> u32 {
                    var state = input * 747796405u + 2891336453u;
                    var word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
                    return (word >> 22u) ^ word;
                }
                
                fn random_float(seed: ptr<function, u32>) -> f32 {
                    *seed = pcg_hash(*seed);
                    return f32(*seed) / 4294967296.0;
                }

                @compute @workgroup_size(64)
                fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
                    let path_idx = global_id.x;
                    if (path_idx >= params.num_paths) {
                        return;
                    }
                    
                    var seed = path_idx + 1337u;
                    var price = params.initial_price;
                    
                    for (var i = 0u; i < params.steps; i = i + 1u) {
                        let u1 = random_float(&seed);
                        let u2 = random_float(&seed);
                        // Box-Muller transform
                        let z = sqrt(-2.0 * log(u1)) * cos(6.28318530718 * u2);
                        
                        price = price * exp((params.drift - 0.5 * params.volatility * params.volatility) * params.dt + params.volatility * sqrt(params.dt) * z);
                    }
                    
                    results[path_idx] = price;
                }
            "#)),
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Monte Carlo Pipeline"),
            layout: None,
            module: &shader,
            entry_point: "main",
        });

        Ok(Self { device, queue, pipeline })
    }

    /// Executes thousands of Monte Carlo simulations on the GPU and returns the 99% VaR
    pub async fn compute_monte_carlo_var(&self, params: RiskParams) -> anyhow::Result<f32> {
        let params_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Risk Params Buffer"),
            contents: bytemuck::cast_slice(&[params]),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let results_size = (params.num_paths as usize * std::mem::size_of::<f32>()) as wgpu::BufferAddress;
        
        let results_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Results Buffer"),
            size: results_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let staging_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Staging Buffer"),
            size: results_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = self.pipeline.get_bind_group_layout(0);
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Compute Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: results_buffer.as_entire_binding(),
                },
            ],
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            let workgroups = params.num_paths.div_ceil(64);
            cpass.dispatch_workgroups(workgroups, 1, 1);
        }
        
        encoder.copy_buffer_to_buffer(&results_buffer, 0, &staging_buffer, 0, results_size);
        self.queue.submit(Some(encoder.finish()));

        let buffer_slice = staging_buffer.slice(..);
        let (sender, receiver) = futures_channel::oneshot::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |v| { let _ = sender.send(v); });
        self.device.poll(wgpu::Maintain::Wait);
        
        if let Ok(Ok(())) = receiver.await {
            let data = buffer_slice.get_mapped_range();
            let mut final_prices: Vec<f32> = bytemuck::cast_slice(&data).to_vec();
            drop(data);
            staging_buffer.unmap();
            
            // Calculate 99% VaR (1st percentile)
            final_prices.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let var_idx = (params.num_paths as f32 * 0.01) as usize;
            let worst_case_price = final_prices[var_idx];
            
            let drawdown = (params.initial_price - worst_case_price) / params.initial_price;
            return Ok(drawdown * 100.0); // Return VaR as a percentage
        }

        anyhow::bail!("Failed to read GPU compute results")
    }
}
