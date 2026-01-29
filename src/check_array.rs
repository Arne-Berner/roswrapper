use bke_ccl::*;
use std::iter;
use wgpu::{
    Buffer,
    util::{BufferInitDescriptor, DeviceExt},
};

pub struct CheckArrays {
    input_buffer: Buffer,
    width: u32,
    height: u32,
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl CheckArrays {
    pub async fn new(input: Vec<i8>) -> anyhow::Result<CheckArrays> {
        let image_bytes: Vec<u32> = input.iter().copied().map(|x| x.max(0) as u32).collect();
        println!("image bytes: {:?}", image_bytes);
        let width: u32 = 8;
        let height: u32 = 8;
            print!("\n");
            print!("\n");
            print!("\n");
        for y in 0..height {
            for x in 0..width {
                let idx = y as usize * width as usize + x as usize;
                print!("{} ", image_bytes[idx]);
            }
            print!("\n");
        }
            print!("\n");
            print!("\n");
            print!("\n");
        // The instance is a handle to our GPU
        // BackendBit::PRIMARY => Vulkan + Metal + DX12 + Browser WebGPU
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .unwrap();

        let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();

        let input_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("input"),
            contents: bytemuck::cast_slice(&image_bytes),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::STORAGE,
        });

        Ok(Self {
            input_buffer,
            width,
            height,
            device,
            queue,
        })
    }

    pub async fn compute(&self) -> anyhow::Result<Buffer> {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });
        // TODO I need to change BKE so that it works like compute_visualizer with the bind groups
        let ccl = CCLState::new(
            &self.device,
            &self.queue,
            &self.input_buffer,
            self.width,
            self.height,
        )
        .unwrap();
        let output_buffer = ccl.compute(&mut encoder)?;
        self.queue.submit(iter::once(encoder.finish()));

        Ok(output_buffer)
    }
}
