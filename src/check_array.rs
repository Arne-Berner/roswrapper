use bke_ccl::*;
use flume::bounded;
use std::iter;
use wgpu::{
    Buffer,
    util::{BufferInitDescriptor, DeviceExt},
};

pub struct CheckArrays {
    input_buffer: Buffer,
    width: u32,
    height: u32,
    expected_array: Vec<u32>,
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
        let expected_array: Vec<u32> = vec![
            1, 0, 0, 0, 0, 0, 0, 0,
            0, 1, 1, 1, 1, 0, 0, 0,
            0, 1, 1, 1, 0, 0, 0, 0,
            0, 1, 1, 1, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 55,
        ];
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
            expected_array,
            device,
            queue,
        })
    }

    pub async fn compute(&self) -> anyhow::Result<()> {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });
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
        self.check_array(&output_buffer).await?;

        Ok(())
    }

    async fn check_array(&self, output_buffer: &wgpu::Buffer) -> anyhow::Result<()> {
        let temp_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("temp"),
            size: self.input_buffer.size(),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self.device.create_command_encoder(&Default::default());

        encoder.copy_buffer_to_buffer(
            output_buffer,
            0,
            &temp_buffer,
            0,
            output_buffer.size() - 4u64,
        );

        self.queue.submit([encoder.finish()]);

        {
            // The mapping process is async, so we'll need to create a channel to get
            // the success flag for our mapping
            let (tx, rx) = bounded(1);

            // We send the success or failure of our mapping via a callback
            temp_buffer.map_async(wgpu::MapMode::Read, .., move |result| {
                tx.send(result).unwrap()
            });

            // The callback we submitted to map async will only get called after the
            // device is polled or the queue submitted
            self.device.poll(wgpu::PollType::wait_indefinitely())?;

            // We check if the mapping was successful here
            rx.recv_async().await??;

            // We then get the bytes that were stored in the buffer
            let output_buffer_view = temp_buffer.get_mapped_range(..);

            // Now we have the data on the CPU we can do what ever we want to with it
            let output_data = bytemuck::cast_slice::<_, u32>(&output_buffer_view);
            for y in 0..self.height {
                for x in 0..self.width {
                    let idx = y as usize * self.width as usize + x as usize;
                    print!("{} ", output_data[idx]);
                }
                print!("\n");
            }

            assert_eq!(output_data, self.expected_array);
        }

        // We need to unmap the buffer to be able to use it again
        temp_buffer.unmap();
        Ok(())
    }
}
